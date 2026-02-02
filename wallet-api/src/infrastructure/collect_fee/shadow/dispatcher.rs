// collect_fee/shadow/dispatcher.rs
use std::{sync::Arc, time::Duration};

use dashmap::DashSet;
use tokio::sync::Semaphore;
use tracing::{debug, error, info};
use wallet_database::CollectDbPool;

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
    RecoverTx(String),
    SendTxAck(String),
    SendTxResAck(String),
    UploadTxExecReceipt(String),
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
            FeeIntent::Chain(FeeChainIntent::RecoverTx(trade_no)) => {
                RunningKey::RecoverTx(trade_no.clone())
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
        }
    }
}

/// RunningGuard 用于 RAII 方式管理 running 标记
/// 确保无论执行路径如何，running 标记都会被正确释放
pub struct RunningGuard {
    key: RunningKey,
    running_set: Arc<DashSet<RunningKey>>,
}

impl RunningGuard {
    /// 创建一个新的 RunningGuard
    /// 注意：调用者需要确保 key 已经被插入到 running_set 中
    pub fn new(key: RunningKey, running_set: Arc<DashSet<RunningKey>>) -> Self {
        Self { key, running_set }
    }
}

impl Drop for RunningGuard {
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
    running: Arc<DashSet<RunningKey>>,
    /// 全局并发控制信号量
    semaphore: Arc<Semaphore>,
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
        let semaphore_size = config.semaphore_size;
        Self {
            pool,
            config,
            running: Arc::new(DashSet::new()),
            semaphore: Arc::new(Semaphore::new(semaphore_size)),
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
            FeeIntent::Chain(FeeChainIntent::RecoverTx(trade_no)) => trade_no.clone(),
            FeeIntent::SideEffect(FeeSideEffectIntent::SendTxResAck(trade_no)) => trade_no.clone(),
            FeeIntent::SideEffect(FeeSideEffectIntent::UploadTxExecReceipt(trade_no)) => {
                trade_no.clone()
            }
            FeeIntent::SideEffect(FeeSideEffectIntent::SendTxAck(trade_no)) => trade_no.clone(),
        };

        info!(?intent, trade_no = %trade_no, "Received fee intent");

        // 1. 从intent生成对应的RunningKey
        let running_key = RunningKey::from_intent(&intent);

        // 2. 检查是否正在执行同一类型的intent
        if !self.running.insert(running_key.clone()) {
            debug!(key = ?running_key, "Running key already in running set, skipping");
            return Ok(());
        }

        // 3. 克隆需要的字段，用于 spawn 的任务中
        let running = self.running.clone();
        let semaphore = self.semaphore.clone();
        let shadow_worker = self.shadow_worker.clone();
        let side_effect_worker = self.side_effect_worker.clone();

        // 4. Spawn 任务执行，实现并发
        tokio::spawn(async move {
            // 获取信号量许可
            let _permit = match semaphore.acquire_owned().await {
                Ok(p) => p,
                Err(_) => {
                    // 信号量已关闭，释放 running 标记并返回
                    running.remove(&running_key);
                    return;
                }
            };

            // 创建 RunningGuard，确保无论如何都会释放 running 标记
            let _guard = RunningGuard::new(running_key, running);

            // 路由 Intent 到正确的 Worker
            match intent {
                FeeIntent::Chain(FeeChainIntent::BuildTx(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending BuildTx command to Shadow Worker");
                    if let Err(e) =
                        shadow_worker.handle(ShadowFeeCommand::BuildTx(trade_no.clone())).await
                    {
                        error!(error = ?e, "Worker execution failed");
                    }
                }
                FeeIntent::Chain(FeeChainIntent::BroadcastTx(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending Broadcast command to Shadow Worker");
                    if let Err(e) =
                        shadow_worker.handle(ShadowFeeCommand::Broadcast(trade_no.clone())).await
                    {
                        error!(error = ?e, "Worker execution failed");
                    }
                }
                FeeIntent::Chain(FeeChainIntent::RecoverTx(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending Recover command to Shadow Worker");
                    if let Err(e) =
                        shadow_worker.handle(ShadowFeeCommand::Recover(trade_no.clone())).await
                    {
                        error!(error = ?e, "Worker execution failed");
                    }
                }
                FeeIntent::SideEffect(FeeSideEffectIntent::SendTxAck(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending SendTxAck command to SideEffect Worker");
                    side_effect_worker
                        .handle(SideEffectCommand::SendOrderAck(trade_no.clone()))
                        .await;
                }
                FeeIntent::SideEffect(FeeSideEffectIntent::SendTxResAck(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending SendTxResAck command to SideEffect Worker");
                    side_effect_worker
                        .handle(SideEffectCommand::SendResultAck(trade_no.clone()))
                        .await;
                }
                FeeIntent::SideEffect(FeeSideEffectIntent::UploadTxExecReceipt(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending UploadTxExecReceipt command to SideEffect Worker");
                    side_effect_worker
                        .handle(SideEffectCommand::UploadTxExecReceipt(trade_no.clone()))
                        .await;
                }
            }
        });

        // 快速返回，Dispatcher 不 await 任务执行
        Ok(())
    }
}
