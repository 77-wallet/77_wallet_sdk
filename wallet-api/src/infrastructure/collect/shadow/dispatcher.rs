// collect/shadow/dispatcher.rs
use std::{sync::Arc, time::Duration, time::Instant};

use dashmap::{DashSet, DashMap};
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};
use wallet_database::CollectDbPool;

use crate::infrastructure::collect::shadow::{
    ChainIntent, SideEffectIntent,
    worker::{ShadowCollectCommand, ShadowCollectWorker, SideEffectCommand, SideEffectWorker},
};

use super::CollectIntent;

/// RunningKey 表示当前正在执行的 intent 的唯一标识
/// 用于 trade_no + intent_type 级别的互斥执行
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum RunningKey {
    BuildTx(String),
    BroadcastTx(String),
    RecoverTx(String),
    SendOrderAck(String),
    SendResultAck(String),
    UploadServiceFee(String),
    UploadTxExecReceipt(String),
    SendTxFeeResAck(String),
}

impl RunningKey {
    /// 从 CollectIntent 生成对应的 RunningKey
    pub fn from_intent(intent: &CollectIntent) -> Self {
        match intent {
            CollectIntent::Chain(ChainIntent::BuildTx(trade_no)) => {
                RunningKey::BuildTx(trade_no.clone())
            }
            CollectIntent::Chain(ChainIntent::BroadcastTx(trade_no)) => {
                RunningKey::BroadcastTx(trade_no.clone())
            }
            CollectIntent::Chain(ChainIntent::RecoverTx(trade_no)) => {
                RunningKey::RecoverTx(trade_no.clone())
            }
            CollectIntent::SideEffect(SideEffectIntent::SendOrderAck(trade_no)) => {
                RunningKey::SendOrderAck(trade_no.clone())
            }
            CollectIntent::SideEffect(SideEffectIntent::SendResultAck(trade_no)) => {
                RunningKey::SendResultAck(trade_no.clone())
            }
            CollectIntent::SideEffect(SideEffectIntent::UploadServiceFee(trade_no)) => {
                RunningKey::UploadServiceFee(trade_no.clone())
            }
            CollectIntent::SideEffect(SideEffectIntent::UploadTxExecReceipt(trade_no)) => {
                RunningKey::UploadTxExecReceipt(trade_no.clone())
            }
            CollectIntent::SideEffect(SideEffectIntent::SendTxFeeResAck(trade_no)) => {
                RunningKey::SendTxFeeResAck(trade_no.clone())
            }
        }
    }
}

/// RunningGuard 用于 RAII 方式管理 running 标记
/// 确保无论执行路径如何，running 标记都会被正确释放
pub struct RunningGuard {
    key: RunningKey,
    running_set: Arc<DashSet<RunningKey>>,
    running_times: Arc<DashMap<RunningKey, Instant>>,
}

impl RunningGuard {
    /// 创建一个新的 RunningGuard
    /// 注意：调用者需要确保 key 已经被插入到 running_set 中
    pub fn new(key: RunningKey, running_set: Arc<DashSet<RunningKey>>, running_times: Arc<DashMap<RunningKey, Instant>>) -> Self {
        Self { key, running_set, running_times }
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        // 无论执行结果如何，都会释放 running 标记
        self.running_set.remove(&self.key);
        self.running_times.remove(&self.key);
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
/// 3. 路由意图到正确的Worker（Shadow Worker 或 SideEffect Worker）
/// 4. 监控长时间运行的任务（Watchdog Scanner）
pub(crate) struct ShadowDispatcher {
    pool: CollectDbPool,
    config: DispatcherConfig,
    /// 正在执行的intent的唯一标识集合，防止并发重复执行同一trade_no的同一intent类型
    running: Arc<DashSet<RunningKey>>,
    /// 运行中任务的开始时间，用于 Watchdog Scanner
    running_times: Arc<DashMap<RunningKey, Instant>>,
    /// 全局并发控制信号量
    semaphore: Arc<Semaphore>,
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
        let semaphore_size = config.semaphore_size;
        Self {
            pool,
            config,
            running: Arc::new(DashSet::new()),
            running_times: Arc::new(DashMap::new()),
            semaphore: Arc::new(Semaphore::new(semaphore_size)),
            shadow_worker,
            side_effect_worker,
            intent_tx,
        }
    }

    /// 处理推进意图
    ///
    /// 注意：
    /// - Scanner 是纯事实扫描器，不依赖事件、时间或触发源
    /// - Scanner 只需要两种入口：
    ///   1. 周期性全量/分段扫描 (scan_round)
    ///   2. 点对点唤醒 (try_advance)
    /// - 扫描是只读的，不应该参与并发控制
    /// - 并发互斥只存在于执行阶段
    pub async fn handle_intent(&self, intent: CollectIntent) -> Result<(), anyhow::Error> {
        // 提取 trade_no
        let trade_no = match &intent {
            CollectIntent::Chain(ChainIntent::BuildTx(trade_no)) => trade_no.clone(),
            CollectIntent::Chain(ChainIntent::BroadcastTx(trade_no)) => trade_no.clone(),
            CollectIntent::Chain(ChainIntent::RecoverTx(trade_no)) => trade_no.clone(),
            CollectIntent::SideEffect(SideEffectIntent::SendResultAck(trade_no)) => {
                trade_no.clone()
            }
            CollectIntent::SideEffect(SideEffectIntent::UploadServiceFee(trade_no)) => {
                trade_no.clone()
            }
            CollectIntent::SideEffect(SideEffectIntent::UploadTxExecReceipt(trade_no)) => {
                trade_no.clone()
            }
            CollectIntent::SideEffect(SideEffectIntent::SendOrderAck(trade_no)) => trade_no.clone(),
            CollectIntent::SideEffect(SideEffectIntent::SendTxFeeResAck(trade_no)) => {
                trade_no.clone()
            }
        };

        info!(?intent, trade_no = %trade_no, "Received collect intent");

        // 1. 从intent生成对应的RunningKey
        let running_key = RunningKey::from_intent(&intent);

        // 2. 检查是否正在执行同一类型的intent
        if !self.running.insert(running_key.clone()) {
            debug!(key = ?running_key, "Running key already in running set, skipping");
            return Ok(());
        }

        // 3. 记录任务开始时间
        self.running_times.insert(running_key.clone(), Instant::now());

        // 4. 克隆需要的字段，用于 spawn 的任务中
        let running = self.running.clone();
        let running_times = self.running_times.clone();
        let semaphore = self.semaphore.clone();
        let shadow_worker = self.shadow_worker.clone();
        let side_effect_worker = self.side_effect_worker.clone();

        // 4. Spawn 任务执行，实现并发
        tokio::spawn(async move {
            // 获取信号量许可
            let start = std::time::Instant::now();
            let _permit = match semaphore.acquire_owned().await {
                Ok(p) => {
                    let acquire_duration = start.elapsed();
                    debug!(key = ?running_key, duration = ?acquire_duration, "Acquired semaphore permit");
                    p
                }
                Err(_) => {
                    // 信号量已关闭，释放 running 标记并返回
                    running.remove(&running_key);
                    return;
                }
            };

            // 创建 RunningGuard，确保无论如何都会释放 running 标记
            let _guard = RunningGuard::new(running_key, running, running_times);

            // 路由 Intent 到正确的 Worker
            if let Err(e) = match intent {
                CollectIntent::Chain(ChainIntent::BuildTx(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending BuildTx command to Shadow Worker");
                    shadow_worker.handle(ShadowCollectCommand::BuildTx(trade_no.clone())).await
                }
                CollectIntent::Chain(ChainIntent::BroadcastTx(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending Broadcast command to Shadow Worker");
                    shadow_worker.handle(ShadowCollectCommand::Broadcast(trade_no.clone())).await
                }
                CollectIntent::Chain(ChainIntent::RecoverTx(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending Recover command to Shadow Worker");
                    shadow_worker.handle(ShadowCollectCommand::Recover(trade_no.clone())).await
                }
                CollectIntent::SideEffect(SideEffectIntent::SendOrderAck(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending SendOrderAck command to SideEffect Worker");
                    side_effect_worker
                        .handle(SideEffectCommand::SendOrderAck(trade_no.clone()))
                        .await
                }
                CollectIntent::SideEffect(SideEffectIntent::SendResultAck(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending SendResultAck command to SideEffect Worker");
                    side_effect_worker
                        .handle(SideEffectCommand::SendResultAck(trade_no.clone()))
                        .await
                }
                CollectIntent::SideEffect(SideEffectIntent::UploadServiceFee(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending UploadServiceFee command to SideEffect Worker");
                    side_effect_worker
                        .handle(SideEffectCommand::UploadServiceFee(trade_no.clone()))
                        .await
                }
                CollectIntent::SideEffect(SideEffectIntent::UploadTxExecReceipt(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending UploadTxExecReceipt command to SideEffect Worker");
                    side_effect_worker
                        .handle(SideEffectCommand::UploadTxExecReceipt(trade_no.clone()))
                        .await
                }
                CollectIntent::SideEffect(SideEffectIntent::SendTxFeeResAck(trade_no)) => {
                    info!(trade_no = %trade_no, "Sending SendTxFeeResAck command to SideEffect Worker");
                    side_effect_worker
                        .handle(SideEffectCommand::SendTxFeeResAck(trade_no.clone()))
                        .await
                }
            } {
                error!(error = ?e, "Worker execution failed");
            }
        });

        // 快速返回，Dispatcher 不 await 任务执行
        Ok(())
    }

    /// Watchdog Scanner 方法
    /// 定期检查长时间运行的任务
    pub(crate) async fn watchdog_scan(&self) {
        let now = Instant::now();
        let mut to_remove = Vec::new();

        // 遍历所有运行中的任务
        for entry in self.running_times.iter() {
            let (key, start_time) = entry.pair();
            let duration = now.duration_since(*start_time);

            match duration.as_secs() {
                60..=119 => {
                    // 60秒：warn
                    warn!(key = ?key, duration = ?duration, "Watchdog: Task running for more than 60 seconds");
                }
                120..=179 => {
                    // 120秒：error
                    error!(key = ?key, duration = ?duration, "Watchdog: Task running for more than 120 seconds");
                }
                180.. => {
                    // 180秒：强制释放
                    error!(key = ?key, duration = ?duration, "Watchdog: Forcing release of task running for more than 180 seconds");
                    to_remove.push(key.clone());
                }
                _ => {
                    // 正常运行时间，忽略
                }
            }
        }

        // 强制释放长时间运行的任务
        for key in to_remove {
            self.running.remove(&key);
            self.running_times.remove(&key);
            error!(key = ?key, "Watchdog: Forcibly released long-running task");
        }
    }
}
