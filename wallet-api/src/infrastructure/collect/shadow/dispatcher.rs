// collect/shadow/dispatcher.rs
use std::{sync::Arc, time::Duration};

use dashmap::DashSet;
use tracing::{debug, info, warn};
use wallet_database::CollectDbPool;

use wallet_database::{
    entities::api_collect::ApiCollectStatus, repositories::api_wallet::collect::ApiCollectRepo,
};

use crate::infrastructure::collect::shadow::{ChainIntent, SideEffectIntent};
use crate::infrastructure::collect::shadow::worker::{ShadowCollectCommand, ShadowCollectWorker, SideEffectCommand, SideEffectWorker};

use super::CollectIntent;

/// RunningKey 表示当前正在执行的 intent 的唯一标识
/// 用于 trade_no + intent_type 级别的互斥执行
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum RunningKey {
    BuildTx(String),
    BroadcastTx(String),
    SendOrderAck(String),
    SendResultAck(String),
    UploadServiceFee(String),
    UploadTxExecReceipt(String),
    /// Tick 意图的运行键
    Tick(String),
}

impl RunningKey {
    /// 从 CollectIntent 生成对应的 RunningKey
    pub fn from_intent(intent: &CollectIntent) -> Self {
        match intent {
            CollectIntent::Chain(ChainIntent::BuildTx(trade_no)) => RunningKey::BuildTx(trade_no.clone()),
            CollectIntent::Chain(ChainIntent::BroadcastTx(trade_no)) => RunningKey::BroadcastTx(trade_no.clone()),
            CollectIntent::SideEffect(SideEffectIntent::SendOrderAck(trade_no)) => RunningKey::SendOrderAck(trade_no.clone()),
            CollectIntent::SideEffect(SideEffectIntent::SendResultAck(trade_no)) => RunningKey::SendResultAck(trade_no.clone()),
            CollectIntent::SideEffect(SideEffectIntent::UploadServiceFee(trade_no)) => RunningKey::UploadServiceFee(trade_no.clone()),
            CollectIntent::SideEffect(SideEffectIntent::UploadTxExecReceipt(trade_no)) => RunningKey::UploadTxExecReceipt(trade_no.clone()),
            CollectIntent::Tick { trade_no } => RunningKey::Tick(trade_no.clone()),
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
    /// 正在执行的trade_no集合，防止并发执行同一trade_no的不同intent类型
    trade_no_running: DashSet<String>,
    /// Shadow Worker，处理链相关操作
    shadow_worker: Arc<ShadowCollectWorker>,
    /// SideEffect Worker，处理外部依赖的副作用操作
    side_effect_worker: Arc<SideEffectWorker>,
    /// 意图发送器，用于 try_advance 生成的意图
    intent_tx: tokio::sync::mpsc::Sender<CollectIntent>,
}

impl ShadowDispatcher {
    pub(crate) fn new(
        pool: CollectDbPool,
        config: DispatcherConfig,
        shadow_worker: Arc<ShadowCollectWorker>,
        side_effect_worker: Arc<SideEffectWorker>,
        intent_tx: tokio::sync::mpsc::Sender<CollectIntent>,
    ) -> Self {
        Self { 
            pool, 
            config, 
            running: DashSet::new(), 
            trade_no_running: DashSet::new(),
            shadow_worker, 
            side_effect_worker,
            intent_tx,
        }
    }

    /// 处理推进意图
    pub async fn handle_intent(&self, intent: CollectIntent) -> Result<(), anyhow::Error> {
        let trade_no = match &intent {
            CollectIntent::Chain(ChainIntent::BuildTx(trade_no)) => trade_no.clone(),
            CollectIntent::Chain(ChainIntent::BroadcastTx(trade_no)) => trade_no.clone(),
            CollectIntent::SideEffect(SideEffectIntent::SendResultAck(trade_no)) => trade_no.clone(),
            CollectIntent::SideEffect(SideEffectIntent::UploadServiceFee(trade_no)) => trade_no.clone(),
            CollectIntent::SideEffect(SideEffectIntent::UploadTxExecReceipt(trade_no)) => trade_no.clone(),
            CollectIntent::SideEffect(SideEffectIntent::SendOrderAck(trade_no)) => trade_no.clone(),
            CollectIntent::Tick { trade_no } => trade_no.clone(),
        };

        info!(?intent, trade_no = %trade_no, "Received collect intent");

        // 1. 检查是否正在执行同一trade_no的任何intent
        if !self.trade_no_running.insert(trade_no.clone()) {
            debug!(trade_no = %trade_no, "Trade_no already in running set, skipping");
            return Ok(());
        }

        // 2. 从intent生成对应的RunningKey
        let running_key = RunningKey::from_intent(&intent);

        // 3. 先进行DB状态二次校验，减少不必要的running占用
        let should_proceed = match self.check_db_state(&intent).await {
            Ok(should) => should,
            Err(e) => {
                warn!(trade_no = %trade_no, error = %e, "DB state check failed");
                // 释放trade_no running标记
                self.trade_no_running.remove(&trade_no);
                return Err(e);
            }
        };

        if !should_proceed {
            info!(trade_no = %trade_no, "DB state not match expected, skipping");
            // 释放trade_no running标记
            self.trade_no_running.remove(&trade_no);
            return Ok(());
        }

        // 4. 检查是否正在执行同一类型的intent
        if !self.running.insert(running_key.clone()) {
            debug!(key = ?running_key, "Running key already in running set, skipping");
            // 释放trade_no running标记
            self.trade_no_running.remove(&trade_no);
            return Ok(());
        }

        // 5. 创建RunningGuard，确保无论如何都会释放running标记
        let _running_guard = RunningGuard::new(running_key.clone(), &self.running);
        // 创建TradeNoRunningGuard，确保无论如何都会释放trade_no running标记
        struct TradeNoRunningGuard<'a> {
            trade_no: String,
            running_set: &'a DashSet<String>,
        }
        impl<'a> Drop for TradeNoRunningGuard<'a> {
            fn drop(&mut self) {
                self.running_set.remove(&self.trade_no);
                debug!(trade_no = %self.trade_no, "Released trade_no running guard");
            }
        }
        let _trade_no_running_guard = TradeNoRunningGuard {
            trade_no: trade_no.clone(),
            running_set: &self.trade_no_running,
        };

        // 4. 路由Intent到正确的Worker
        match intent {
            CollectIntent::Chain(ChainIntent::BuildTx(trade_no)) => {
                info!(trade_no = %trade_no, "Sending BuildTx command to Shadow Worker");
                self.shadow_worker
                    .handle(ShadowCollectCommand::BuildTx(trade_no.clone()))
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to handle BuildTx intent: {}", e))?;
            }
            CollectIntent::Chain(ChainIntent::BroadcastTx(trade_no)) => {
                info!(trade_no = %trade_no, "Sending Broadcast command to Shadow Worker");
                self.shadow_worker
                    .handle(ShadowCollectCommand::Broadcast(trade_no.clone()))
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to handle Broadcast intent: {}", e))?;
            }
            CollectIntent::SideEffect(SideEffectIntent::SendOrderAck(trade_no)) => {
                info!(trade_no = %trade_no, "Sending SendOrderAck command to SideEffect Worker");
                self.side_effect_worker
                    .handle(SideEffectCommand::SendOrderAck(trade_no.clone()))
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to handle SendOrderAck intent: {}", e))?;
            }
            CollectIntent::SideEffect(SideEffectIntent::SendResultAck(trade_no)) => {
                info!(trade_no = %trade_no, "Sending SendResultAck command to SideEffect Worker");
                self.side_effect_worker
                    .handle(SideEffectCommand::SendResultAck(trade_no.clone()))
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to handle SendResultAck intent: {}", e))?;
            }
            CollectIntent::SideEffect(SideEffectIntent::UploadServiceFee(trade_no)) => {
                info!(trade_no = %trade_no, "Sending UploadServiceFee command to SideEffect Worker");
                self.side_effect_worker
                    .handle(SideEffectCommand::UploadServiceFee(trade_no.clone()))
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to handle UploadServiceFee intent: {}", e))?;
            }
            CollectIntent::SideEffect(SideEffectIntent::UploadTxExecReceipt(trade_no)) => {
                info!(trade_no = %trade_no, "Sending UploadTxExecReceipt command to SideEffect Worker");
                self.side_effect_worker
                    .handle(SideEffectCommand::UploadTxExecReceipt(trade_no.clone()))
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to handle UploadTxExecReceipt intent: {}", e))?;
            }
            CollectIntent::Tick { trade_no } => {
                info!(trade_no = %trade_no, "Handling Tick intent, calling try_advance");
                // 创建一个临时的 ShadowScanner 实例来处理 try_advance
                let scanner = crate::infrastructure::collect::shadow::ShadowScanner::new(
                    self.pool.clone(),
                    crate::infrastructure::collect::shadow::ScannerConfig::default(),
                    self.intent_tx.clone(),
                );
                // 调用 try_advance 处理 Tick 意图
                scanner.try_advance(&trade_no).await;
            }
        }

        Ok(())
    }

    /// 检查DB状态是否符合预期
    async fn check_db_state(&self, intent: &CollectIntent) -> Result<bool, anyhow::Error> {
        let trade_no = match intent {
            CollectIntent::Chain(ChainIntent::BuildTx(trade_no)) => trade_no,
            CollectIntent::Chain(ChainIntent::BroadcastTx(trade_no)) => trade_no,
            CollectIntent::SideEffect(SideEffectIntent::SendResultAck(trade_no)) => trade_no,
            CollectIntent::SideEffect(SideEffectIntent::UploadServiceFee(trade_no)) => trade_no,
            CollectIntent::SideEffect(SideEffectIntent::UploadTxExecReceipt(trade_no)) => trade_no,
            CollectIntent::SideEffect(SideEffectIntent::SendOrderAck(trade_no)) => trade_no,
            CollectIntent::Tick { trade_no } => trade_no,
        };

        // 查询最新的DB状态，添加超时保护
        let collect = tokio::time::timeout(
            self.config.db_check_timeout,
            ApiCollectRepo::get_api_collect_by_trade_no(&self.pool, trade_no),
        )
        .await
        .map_err(|_| anyhow::anyhow!("dispatcher db_check timeout, trade_no={}", trade_no))?
        .map_err(|e| anyhow::anyhow!("Failed to get api collect by trade_no: {}", e))?;

        // 根据意图检查状态是否符合预期
        match intent {
            CollectIntent::Chain(ChainIntent::BuildTx(_)) => {
                // INIT状态才需要BuildTx
                Ok(collect.status == ApiCollectStatus::Init)
            }
            CollectIntent::Chain(ChainIntent::BroadcastTx(_)) => {
                // SENDING状态才需要Broadcast
                Ok(collect.status == ApiCollectStatus::SendingTx)
            }
            CollectIntent::SideEffect(SideEffectIntent::SendOrderAck(_)) => {
                // 检查是否满足发送 Order ACK 的条件
                // ⚠️ 只看推进事实，不看行为事实：
                // - order_ack_sent_at IS NULL：尚未发送订单确认（推进事实）
                //
                // ❌ 不检查 order_ack_attempted_at（这是行为事实，不参与判断）
                Ok(collect.order_ack_sent_at.is_none())
            }
            CollectIntent::SideEffect(SideEffectIntent::SendResultAck(_)) => {
                // 检查是否满足发送 Result ACK 的条件
                // ⚠️ 只看推进事实，不看行为事实：
                // - transaction_time IS NOT NULL：链上已给出结果
                // - result_ack_sent_at IS NULL：尚未发送结果确认（推进事实）
                //
                // ❌ 不检查 result_ack_attempted_at（这是行为事实，不参与判断）
                // ❌ 不检查 finished_at（这是链事实完成，不表示副作用完成）
                Ok(collect.transaction_time.is_some()
                    && collect.result_ack_sent_at.is_none())
            }
            CollectIntent::SideEffect(SideEffectIntent::UploadServiceFee(_)) => {
                // 检查是否满足上传服务费的条件
                // ⚠️ 只看链事实和副作用事实：
                // - transaction_time IS NOT NULL：链上已给出结果（基于已确认的链事实）
                // - service_fee_uploaded_at IS NULL：尚未上传服务费
                //
                // ❌ 不检查 finished_at（这是链事实完成，不表示副作用完成）
                Ok(collect.transaction_time.is_some()
                    && collect.service_fee_uploaded_at.is_none())
            }
            CollectIntent::SideEffect(SideEffectIntent::UploadTxExecReceipt(_)) => {
                // 检查是否满足上传交易执行回执的条件
                // ⚠️ 只看链事实和副作用事实：
                // - transaction_time IS NOT NULL：链上已给出结果（基于已确认的链事实）
                // - tx_exec_receipt_uploaded_at IS NULL：尚未上传执行回执
                //
                // ❌ 不检查 finished_at（这是链事实完成，不表示副作用完成）
                Ok(collect.transaction_time.is_some()
                    && collect.tx_exec_receipt_uploaded_at.is_none())
            }
            CollectIntent::Tick { .. } => {
                // Tick 意图总是允许执行，因为 try_advance 会自己检查所有事实状态
                Ok(true)
            }
        }
    }
}
