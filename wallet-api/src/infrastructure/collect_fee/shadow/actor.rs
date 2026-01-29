// collect_fee/shadow/actor.rs
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, error, info};
use wallet_database::{CollectDbPool, CoreDbPool};

use crate::infrastructure::collect_fee::{
    process_fee_tx_send::AddressLockManager,
    shadow::worker::{ShadowFeeWorker, SideEffectWorker},
};

use super::dispatcher::ShadowDispatcher;

use super::{DispatcherConfig, FeeIntent, ScannerConfig, ShadowScanner};

/// Dispatcher Actor 消息
#[derive(Debug)]
pub enum DispatcherActorMessage {
    /// 处理推进意图
    HandleIntent(FeeIntent),
}

/// Scanner Actor
pub struct FeeShadowScannerActor {
    pool: CollectDbPool,
    config: ScannerConfig,
    intent_tx: mpsc::Sender<FeeIntent>,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
}

impl FeeShadowScannerActor {
    pub fn new(
        pool: CollectDbPool,
        config: ScannerConfig,
        intent_tx: mpsc::Sender<FeeIntent>,
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Self {
        Self { pool, config, intent_tx, shutdown_rx }
    }

    pub async fn run(mut self) {
        info!("Fee Shadow Scanner Actor running");

        // 创建Scanner实例
        let scanner =
            ShadowScanner::new(self.pool.clone(), self.config.clone(), self.intent_tx.clone());

        // 自定义扫描循环，支持shutdown信号
        let mut interval = tokio::time::interval(scanner.config.scan_interval);
        loop {
            tokio::select! {
                // 接收关闭信号
                _ = self.shutdown_rx.recv() => {
                    info!("Received shutdown signal for Scanner Actor");
                    break;
                },
                // 定时执行扫描
                _ = interval.tick() => {
                    // scan_round is intentionally sequential; overlapping scans are forbidden
                    scanner.scan_round().await;
                },
            }
        }

        info!("Fee Shadow Scanner Actor stopped");
    }
}

/// Dispatcher Actor
pub struct FeeShadowDispatcherActor {
    pool: CollectDbPool,
    config: DispatcherConfig,
    shadow_worker: Arc<ShadowFeeWorker>,
    side_effect_worker: Arc<SideEffectWorker>,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    message_rx: mpsc::Receiver<DispatcherActorMessage>,
    /// 意图发送器，用于 try_advance 生成的意图
    intent_tx: mpsc::Sender<FeeIntent>,
}

impl FeeShadowDispatcherActor {
    pub fn new(
        pool: CollectDbPool,
        config: DispatcherConfig,
        shadow_worker: Arc<ShadowFeeWorker>,
        side_effect_worker: Arc<SideEffectWorker>,
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
        message_rx: mpsc::Receiver<DispatcherActorMessage>,
        intent_tx: mpsc::Sender<FeeIntent>,
    ) -> Self {
        Self { pool, config, shadow_worker, side_effect_worker, shutdown_rx, message_rx, intent_tx }
    }

    pub async fn run(mut self) {
        info!("Fee Shadow Dispatcher Actor running");

        // 创建唯一的ShadowDispatcher实例
        let dispatcher = ShadowDispatcher::new(
            self.pool.clone(),
            self.config.clone(),
            self.shadow_worker.clone(),
            self.side_effect_worker.clone(),
            self.intent_tx.clone(),
        );
        // 用Arc包装，方便在spawn的任务中使用
        let dispatcher = Arc::new(dispatcher);

        // 创建Semaphore和JoinSet
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.config.semaphore_size));
        let mut join_set = tokio::task::JoinSet::new();

        loop {
            tokio::select! {
                // 接收关闭信号
                _ = self.shutdown_rx.recv() => {
                    info!("Received shutdown signal for Dispatcher Actor");
                    break;
                },
                // 接收消息
                msg = self.message_rx.recv() => {
                    match msg {
                        Some(DispatcherActorMessage::HandleIntent(intent)) => {
                            // 处理意图（在新任务中运行，避免阻塞Actor消息处理）
                            let dispatcher_clone = dispatcher.clone();
                            let intent_clone = intent.clone();
                            let semaphore_clone = semaphore.clone();

                            // 区分 Tick 和其他意图的处理
                            match &intent {
                                FeeIntent::Tick { .. } => {
                                    // Tick 是机会性推进，使用 try_acquire
                                    if let Ok(permit) = semaphore_clone.try_acquire_owned() {
                                        // 使用JoinSet管理任务
                                        join_set.spawn(async move {
                                            let _permit = permit;
                                            // Tick 失败不记录错误，因为是机会性的
                                            let _ = dispatcher_clone.handle_intent(intent_clone).await;
                                        });
                                    } else {
                                        // Tick 获取不到许可，直接跳过
                                        debug!("Tick skipped due to semaphore busy: {:?}", intent);
                                    }
                                }
                                _ => {
                                    // 非 Tick 意图，使用正常的 acquire
                                    let permit = match semaphore_clone.acquire_owned().await {
                                        Ok(permit) => permit,
                                        Err(_) => {
                                            error!("Semaphore closed, skipping intent: {:?}", intent_clone);
                                            continue;
                                        }
                                    };

                                    // 使用JoinSet管理任务
                                    join_set.spawn(async move {
                                        let _permit = permit;
                                        if let Err(e) = dispatcher_clone.handle_intent(intent_clone).await {
                                            error!("Failed to handle intent: {}", e);
                                        }
                                    });
                                }
                            }
                        },
                        None => {
                            info!("Dispatcher Actor message channel closed");
                            break;
                        },
                    }

                    // 非阻塞清理已完成任务
                    while let Some(res) = join_set.try_join_next() {
                        if let Err(e) = res {
                            error!("Dispatcher task failed: {}", e);
                        }
                    }
                }
            }
        }

        // 在shutdown时等待所有任务完成
        info!("Waiting for all dispatcher tasks to complete");
        while let Some(res) = join_set.join_next().await {
            if let Err(e) = res {
                error!("Dispatcher task failed: {}", e);
            }
        }

        info!("Fee Shadow Dispatcher Actor stopped");
    }
}

/// Shadow系统Actor管理
#[derive(Debug)]
pub struct FeeShadowActorSystem {
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    dispatcher_message_tx: mpsc::Sender<DispatcherActorMessage>,
    scanner_handle: Option<tokio::task::JoinHandle<()>>,
    dispatcher_handle: Option<tokio::task::JoinHandle<()>>,
    intent_tx: mpsc::Sender<FeeIntent>,
}

impl FeeShadowActorSystem {
    pub fn new(api_funds_pool: CollectDbPool, core_pool: CoreDbPool) -> Self {
        let (shutdown_tx, shutdown_rx1) = tokio::sync::broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();

        let (dispatcher_message_tx, dispatcher_message_rx) = mpsc::channel(100);
        let (intent_tx, mut intent_rx) = mpsc::channel(1000);

        // 创建Scanner Actor
        let scanner_actor = FeeShadowScannerActor::new(
            api_funds_pool.clone(),
            ScannerConfig::default(),
            intent_tx.clone(),
            shutdown_rx1,
        );
        let scanner_handle = Some(tokio::spawn(async move {
            scanner_actor.run().await;
        }));

        // 初始化Shadow Worker
        // 创建AddressLockManager
        let address_locks = Arc::new(AddressLockManager::new());
        // 创建全局信号量，控制RPC/链上执行的并发度
        let global_sem = Arc::new(tokio::sync::Semaphore::new(64));
        // 创建ShadowFeeWorker
        let shadow_worker = Arc::new(ShadowFeeWorker::new(
            api_funds_pool.clone(),
            core_pool.clone(),
            address_locks,
            global_sem,
        ));

        // 初始化SideEffect Worker
        let side_effect_worker =
            Arc::new(SideEffectWorker::new(api_funds_pool.clone(), core_pool.clone()));

        // 创建Dispatcher Actor
        let dispatcher_actor = FeeShadowDispatcherActor::new(
            api_funds_pool,
            DispatcherConfig::default(),
            shadow_worker,
            side_effect_worker,
            shutdown_rx2,
            dispatcher_message_rx,
            intent_tx.clone(),
        );
        let dispatcher_handle = Some(tokio::spawn(async move {
            crate::infrastructure::system_ready::wait_system_ready().await;
            dispatcher_actor.run().await;
        }));

        // 创建意图转发任务（从intent_rx接收意图，发送给Dispatcher Actor）
        let dispatcher_message_tx_clone = dispatcher_message_tx.clone();
        let mut shutdown_rx3 = shutdown_tx.subscribe();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // 接收关闭信号
                    _ = shutdown_rx3.recv() => {
                        info!("Received shutdown signal for intent forward task");
                        break;
                    },
                    // 接收意图
                    Some(intent) = intent_rx.recv() => {
                        if let Err(e) = dispatcher_message_tx_clone
                            .send(DispatcherActorMessage::HandleIntent(intent))
                            .await
                        {
                            error!("Failed to send fee intent to Dispatcher Actor: {}", e);
                        }
                    },
                }
            }
        });

        Self { shutdown_tx, dispatcher_message_tx, scanner_handle, dispatcher_handle, intent_tx }
    }

    /// 停止Shadow系统
    pub async fn stop(&mut self) {
        info!("Stopping Fee Shadow System");

        // 发送停止信号
        let _ = self.shutdown_tx.send(());

        // 等待Actor结束
        if let Some(handle) = self.scanner_handle.take() {
            let _ = handle.await;
        }

        if let Some(handle) = self.dispatcher_handle.take() {
            let _ = handle.await;
        }

        info!("Fee Shadow System stopped");
    }

    /// 获取意图发送器
    pub fn get_intent_tx(&self) -> mpsc::Sender<FeeIntent> {
        self.intent_tx.clone()
    }

    /// 触发一次针对特定 trade_no 的手续费推进
    ///
    /// 语义：
    /// - 有新事实了，立即尝试推进一次
    /// - 不是执行流程，而是提前跑一次 Shadow 的事实驱动推进
    /// - 幂等，多次调用不会导致重复执行
    /// - Tick 是一种低语义、低优先级的推进触发
    /// - 不保证立即执行
    /// - 不保证一定推进
    /// - 只保证"进入 Shadow 的调度视野"
    pub async fn trigger_fee(
        &self,
        trade_no: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        // 生成一个 Tick 意图，让 Shadow 系统尝试推进
        let intent = FeeIntent::Tick { trade_no: trade_no.to_string() };

        // 发送意图到通道
        self.intent_tx.send(intent).await.map_err(|e| {
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::ChannelSendFailed(e.to_string()),
            )
        })?;

        Ok(())
    }
}
