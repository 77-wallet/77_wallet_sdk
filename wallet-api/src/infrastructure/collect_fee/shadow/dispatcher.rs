// collect_fee/shadow/dispatcher.rs
use std::{sync::Arc, time::Duration};

use dashmap::DashSet;
use tracing::{debug, info, warn};
use wallet_database::CollectDbPool;

use wallet_database::repositories::api_wallet::fee::ApiFeeRepo;

use crate::infrastructure::collect_fee::shadow::{
    FeeChainIntent, FeeSideEffectIntent,
    worker::{ShadowFeeCommand, ShadowFeeWorker, SideEffectCommand, SideEffectWorker},
};

use super::FeeIntent;

/// RunningKey 表示当前正在执行的 intent 的唯一标识
/// 用于 trade_no + intent_type 级别的互斥执行
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum RunningKey {
    BuildTx(String),
    BroadcastTx(String),
    SendTxAck(String),
    SendTxResAck(String),
    UploadTxExecReceipt(String),
    /// Tick 意图的运行键
    Tick(String),
}

impl RunningKey {
    /// 从 FeeIntent 生成对应的 RunningKey
    pub fn from_intent(intent: &FeeIntent) -> Self {
        match intent {
            FeeIntent::Chain(FeeChainIntent::BuildTx(trade_no)) => {
                RunningKey::BuildTx(trade_no.clone())
            }
            FeeIntent::Chain(FeeChainIntent::BroadcastTx(trade_no)) => {
                RunningKey::BroadcastTx(trade_no.clone())
            }
            FeeIntent::SideEffect(FeeSideEffectIntent::SendTxAck(trade_no)) => {
                RunningKey::SendTxAck(trade_no.clone())
            }
            FeeIntent::SideEffect(FeeSideEffectIntent::SendTxResAck(trade_no)) => {
                RunningKey::SendTxResAck(trade_no.clone())
            }
            FeeIntent::SideEffect(FeeSideEffectIntent::UploadTxExecReceipt(trade_no)) => {
                RunningKey::UploadTxExecReceipt(trade_no.clone())
            }
            FeeIntent::Tick { trade_no } => RunningKey::Tick(trade_no.clone()),
        }
    }
}

/// RunningGuard 用于 RAII 方式管理 running 标记
/// 确保无论执行路径如何，running 标记都会被正确释放
pub struct RunningGuard<'a> {
    key: RunningKey,
    running_set: &'a DashSet<RunningKey>,
}

impl<'a> RunningGuard<'a> {
    /// 创建一个新的 RunningGuard
    /// 注意：调用者需要确保 key 已经被插入到 running_set 中
    pub fn new(key: RunningKey, running_set: &'a DashSet<RunningKey>) -> Self {
        Self { key, running_set }
    }
}

impl<'a> Drop for RunningGuard<'a> {
    fn drop(&mut self) {
        // 无论执行结果如何，都会释放 running 标记
        self.running_set.remove(&self.key);
        debug!(key = ?self.key, "Released running guard");
    }
}

/// Shadow Dispatcher 配置
#[derive(Debug, Clone)]
pub struct DispatcherConfig {
    /// 全局并发控制信号量大小
    pub semaphore_size: usize,
    /// 二次校验超时时间
    pub db_check_timeout: Duration,
}

impl Default for DispatcherConfig {
    fn default() -> Self {
        Self {
            semaphore_size: 100,
            db_check_timeout: Duration::from_secs(5), // 5秒
        }
    }
}

/// Shadow Dispatcher
///
/// 负责：
/// 1. 防止并发重复执行同一trade_no的同一intent类型
/// 2. 控制全局吞吐
/// 3. DB状态二次校验
/// 4. 决策是否推进状态
/// 5. 路由意图到正确的Worker（Shadow Worker 或 SideEffect Worker）
pub(crate) struct ShadowDispatcher {
    pool: CollectDbPool,
    config: DispatcherConfig,
    /// 正在执行的intent的唯一标识集合，防止并发重复执行同一trade_no的同一intent类型
    running: DashSet<RunningKey>,
    /// Shadow Worker，处理链相关操作
    shadow_worker: Arc<ShadowFeeWorker>,
    /// SideEffect Worker，处理外部依赖的副作用操作
    side_effect_worker: Arc<SideEffectWorker>,
    /// 意图发送器，用于 try_advance 生成的意图
    intent_tx: tokio::sync::mpsc::Sender<FeeIntent>,
}

impl ShadowDispatcher {
    pub(crate) fn new(
        pool: CollectDbPool,
        config: DispatcherConfig,
        shadow_worker: Arc<ShadowFeeWorker>,
        side_effect_worker: Arc<SideEffectWorker>,
        intent_tx: tokio::sync::mpsc::Sender<FeeIntent>,
    ) -> Self {
        Self {
            pool,
            config,
            running: DashSet::new(),
            shadow_worker,
            side_effect_worker,
            intent_tx,
        }
    }

    /// 处理推进意图
    pub async fn handle_intent(&self, intent: FeeIntent) -> Result<(), anyhow::Error> {
        let trade_no = match &intent {
            FeeIntent::Chain(FeeChainIntent::BuildTx(trade_no)) => trade_no.clone(),
            FeeIntent::Chain(FeeChainIntent::BroadcastTx(trade_no)) => trade_no.clone(),
            FeeIntent::SideEffect(FeeSideEffectIntent::SendTxResAck(trade_no)) => trade_no.clone(),
            FeeIntent::SideEffect(FeeSideEffectIntent::UploadTxExecReceipt(trade_no)) => {
                trade_no.clone()
            }
            FeeIntent::SideEffect(FeeSideEffectIntent::SendTxAck(trade_no)) => trade_no.clone(),
            FeeIntent::Tick { trade_no } => trade_no.clone(),
        };

        info!(?intent, trade_no = %trade_no, "Received fee intent");

        // 1. 从intent生成对应的RunningKey
        let running_key = RunningKey::from_intent(&intent);

        // 2. 先进行DB状态二次校验，减少不必要的running占用
        let should_proceed = match self.check_db_state(&intent).await {
            Ok(should) => should,
            Err(e) => {
                warn!(trade_no = %trade_no, error = %e, "DB state check failed");
                return Err(e);
            }
        };

        if !should_proceed {
            info!(trade_no = %trade_no, "DB state not match expected, skipping");
            return Ok(());
        }

        // 3. 检查是否正在执行同一类型的intent
        if !self.running.insert(running_key.clone()) {
            debug!(key = ?running_key, "Running key already in running set, skipping");
            return Ok(());
        }

        // 4. 创建RunningGuard，确保无论如何都会释放running标记
        let _running_guard = RunningGuard::new(running_key.clone(), &self.running);

        // 4. 路由Intent到正确的Worker
        match intent {
            FeeIntent::Chain(FeeChainIntent::BuildTx(trade_no)) => {
                info!(trade_no = %trade_no, "Sending BuildTx command to Shadow Worker");
                self.shadow_worker
                    .handle(ShadowFeeCommand::BuildTx(trade_no.clone()))
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to handle BuildTx intent: {}", e))?;
            }
            FeeIntent::Chain(FeeChainIntent::BroadcastTx(trade_no)) => {
                info!(trade_no = %trade_no, "Sending Broadcast command to Shadow Worker");
                self.shadow_worker
                    .handle(ShadowFeeCommand::Broadcast(trade_no.clone()))
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to handle Broadcast intent: {}", e))?;
            }
            FeeIntent::SideEffect(FeeSideEffectIntent::SendTxAck(trade_no)) => {
                info!(trade_no = %trade_no, "Sending SendTxAck command to SideEffect Worker");
                self.side_effect_worker
                    .handle(SideEffectCommand::SendTxAck(trade_no.clone()))
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to handle SendTxAck intent: {}", e))?;
            }
            FeeIntent::SideEffect(FeeSideEffectIntent::SendTxResAck(trade_no)) => {
                info!(trade_no = %trade_no, "Sending SendTxResAck command to SideEffect Worker");
                self.side_effect_worker
                    .handle(SideEffectCommand::SendTxResAck(trade_no.clone()))
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to handle SendTxResAck intent: {}", e))?;
            }

            FeeIntent::SideEffect(FeeSideEffectIntent::UploadTxExecReceipt(trade_no)) => {
                info!(trade_no = %trade_no, "Sending UploadTxExecReceipt command to SideEffect Worker");
                self.side_effect_worker
                    .handle(SideEffectCommand::UploadTxExecReceipt(trade_no.clone()))
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!("Failed to handle UploadTxExecReceipt intent: {}", e)
                    })?;
            }
            FeeIntent::Tick { trade_no } => {
                info!(trade_no = %trade_no, "Handling Tick intent, calling try_advance");
                // 创建一个临时的 ShadowScanner 实例来处理 try_advance
                let scanner = crate::infrastructure::collect_fee::shadow::ShadowScanner::new(
                    self.pool.clone(),
                    crate::infrastructure::collect_fee::shadow::ScannerConfig::default(),
                    self.intent_tx.clone(),
                );
                // 调用 try_advance 处理 Tick 意图
                scanner.try_advance(&trade_no).await;
            }
        }

        Ok(())
    }

    /// 检查DB状态是否符合预期
    async fn check_db_state(&self, intent: &FeeIntent) -> Result<bool, anyhow::Error> {
        let trade_no = match intent {
            FeeIntent::Chain(FeeChainIntent::BuildTx(trade_no)) => trade_no,
            FeeIntent::Chain(FeeChainIntent::BroadcastTx(trade_no)) => trade_no,
            FeeIntent::SideEffect(FeeSideEffectIntent::SendTxResAck(trade_no)) => trade_no,
            FeeIntent::SideEffect(FeeSideEffectIntent::UploadTxExecReceipt(trade_no)) => trade_no,
            FeeIntent::SideEffect(FeeSideEffectIntent::SendTxAck(trade_no)) => trade_no,
            FeeIntent::Tick { trade_no } => trade_no,
        };

        // 查询最新的DB状态，添加超时保护
        let fee = tokio::time::timeout(
            self.config.db_check_timeout,
            ApiFeeRepo::get_api_fee_by_trade_no(&self.pool, trade_no),
        )
        .await
        .map_err(|_| anyhow::anyhow!("dispatcher db_check timeout, trade_no={}", trade_no))?
        .map_err(|e| anyhow::anyhow!("Failed to get api fee by trade_no: {}", e))?;

        // 根据意图检查状态是否符合预期
        match intent {
            FeeIntent::Chain(FeeChainIntent::BuildTx(_)) => {
                // 检查是否满足构建交易的条件
                // 这里可以根据具体的业务逻辑添加检查
                Ok(true)
            }
            FeeIntent::Chain(FeeChainIntent::BroadcastTx(_)) => {
                // 检查是否满足广播交易的条件
                // 这里可以根据具体的业务逻辑添加检查
                Ok(true)
            }
            FeeIntent::SideEffect(FeeSideEffectIntent::SendTxAck(_)) => {
                // 检查是否满足发送 Tx ACK 的条件
                Ok(fee.tx_ack_sent_at.is_none())
            }
            FeeIntent::SideEffect(FeeSideEffectIntent::SendTxResAck(_)) => {
                // 检查是否满足发送 TxRes ACK 的条件
                // ⚠️ 只看推进事实，不看行为事实：
                // - transaction_time IS NOT NULL：链上已给出结果
                // - tx_res_ack_sent_at IS NULL：尚未发送结果确认（推进事实）
                Ok(fee.transaction_time.is_some() && fee.tx_res_ack_sent_at.is_none())
            }

            FeeIntent::SideEffect(FeeSideEffectIntent::UploadTxExecReceipt(_)) => {
                // 检查是否满足上传交易执行回执的条件
                // ⚠️ 只看链事实和副作用事实：
                // - transaction_time IS NOT NULL：链上已给出结果（基于已确认的链事实）
                Ok(fee.transaction_time.is_some())
            }
            FeeIntent::Tick { .. } => {
                // Tick 意图总是允许执行，因为 try_advance 会自己检查所有事实状态
                Ok(true)
            }
        }
    }
}
