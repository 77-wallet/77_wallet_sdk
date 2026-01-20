use std::{sync::Arc, time::Duration};

use sqlx::SqlitePool;
use tokio::sync::mpsc;
use tracing::{error, info};

use super::{CollectIntent, DispatcherConfig, ScannerConfig, ShadowDispatcher, ShadowScanner};

/// Scanner Actor 消息
#[derive(Debug)]
pub enum ScannerActorMessage {
    /// 启动扫描器
    Start,
    /// 停止扫描器
    Stop,
}

/// Dispatcher Actor 消息
#[derive(Debug)]
pub enum DispatcherActorMessage {
    /// 处理推进意图
    HandleIntent(CollectIntent),
    /// 停止分发器
    Stop,
}

/// Scanner Actor
pub struct CollectorShadowScannerActor {
    pool: Arc<SqlitePool>,
    config: ScannerConfig,
    intent_tx: mpsc::Sender<CollectIntent>,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    message_rx: mpsc::Receiver<ScannerActorMessage>,
}

impl CollectorShadowScannerActor {
    pub fn new(
        pool: Arc<SqlitePool>,
        config: ScannerConfig,
        intent_tx: mpsc::Sender<CollectIntent>,
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
        message_rx: mpsc::Receiver<ScannerActorMessage>,
    ) -> Self {
        Self { pool, config, intent_tx, shutdown_rx, message_rx }
    }

    pub async fn run(mut self) {
        info!("Collector Shadow Scanner Actor running");

        loop {
            tokio::select! {
                // 接收关闭信号
                _ = self.shutdown_rx.recv() => {
                    info!("Received shutdown signal for Scanner Actor");
                    break;
                },
                // 接收消息
                msg = self.message_rx.recv() => {
                    match msg {
                        Some(ScannerActorMessage::Start) => {
                            info!("Starting Scanner Actor scan loop");
                            // 启动扫描循环（在新任务中运行，避免阻塞Actor消息处理）
                            let pool = self.pool.clone();
                            let config = self.config.clone();
                            let intent_tx = self.intent_tx.clone();
                            tokio::spawn(async move {
                                let scanner = ShadowScanner::new(pool, config, intent_tx);
                                scanner.start().await;
                            });
                        },
                        Some(ScannerActorMessage::Stop) => {
                            info!("Stopping Scanner Actor");
                            break;
                        },
                        None => {
                            info!("Scanner Actor message channel closed");
                            break;
                        },
                    }
                },
            }
        }

        info!("Collector Shadow Scanner Actor stopped");
    }
}

/// Dispatcher Actor
pub struct CollectorShadowDispatcherActor {
    pool: Arc<SqlitePool>,
    config: DispatcherConfig,
    tx_tx:
        tokio::sync::mpsc::Sender<crate::infrastructure::collect::command::ProcessCollectTxCommand>,
    report_tx: tokio::sync::mpsc::Sender<
        crate::infrastructure::collect::command::ProcessCollectTxReportCommand,
    >,
    confirm_report_tx: tokio::sync::mpsc::Sender<
        crate::infrastructure::collect::command::ProcessCollectTxConfirmReportCommand,
    >,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    message_rx: mpsc::Receiver<DispatcherActorMessage>,
}

impl CollectorShadowDispatcherActor {
    pub fn new(
        pool: Arc<SqlitePool>,
        config: DispatcherConfig,
        tx_tx: tokio::sync::mpsc::Sender<
            crate::infrastructure::collect::command::ProcessCollectTxCommand,
        >,
        report_tx: tokio::sync::mpsc::Sender<
            crate::infrastructure::collect::command::ProcessCollectTxReportCommand,
        >,
        confirm_report_tx: tokio::sync::mpsc::Sender<
            crate::infrastructure::collect::command::ProcessCollectTxConfirmReportCommand,
        >,
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
        message_rx: mpsc::Receiver<DispatcherActorMessage>,
    ) -> Self {
        Self { pool, config, tx_tx, report_tx, confirm_report_tx, shutdown_rx, message_rx }
    }

    pub async fn run(mut self) {
        info!("Collector Shadow Dispatcher Actor running");

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
                            let pool = self.pool.clone();
                            let config = self.config.clone();
                            let tx_tx = self.tx_tx.clone();
                            let report_tx = self.report_tx.clone();
                            let confirm_report_tx = self.confirm_report_tx.clone();
                            let intent_clone = intent.clone();

                            tokio::spawn(async move {
                                let dispatcher = ShadowDispatcher::new(
                                    pool,
                                    config,
                                    tx_tx,
                                    report_tx,
                                    confirm_report_tx,
                                );
                                if let Err(e) = dispatcher.handle_intent(intent_clone).await {
                                    error!("Failed to handle intent: {}", e);
                                }
                            });
                        },
                        Some(DispatcherActorMessage::Stop) => {
                            info!("Stopping Dispatcher Actor");
                            break;
                        },
                        None => {
                            info!("Dispatcher Actor message channel closed");
                            break;
                        },
                    }
                },
            }
        }

        info!("Collector Shadow Dispatcher Actor stopped");
    }
}

/// Shadow系统Actor管理
#[derive(Debug)]
pub struct CollectorShadowActorSystem {
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    scanner_message_tx: mpsc::Sender<ScannerActorMessage>,
    dispatcher_message_tx: mpsc::Sender<DispatcherActorMessage>,
    scanner_handle: Option<tokio::task::JoinHandle<()>>,
    dispatcher_handle: Option<tokio::task::JoinHandle<()>>,
    intent_tx: mpsc::Sender<CollectIntent>,
}

impl CollectorShadowActorSystem {
    pub fn new(
        pool: Arc<SqlitePool>,
        tx_tx: tokio::sync::mpsc::Sender<
            crate::infrastructure::collect::command::ProcessCollectTxCommand,
        >,
        report_tx: tokio::sync::mpsc::Sender<
            crate::infrastructure::collect::command::ProcessCollectTxReportCommand,
        >,
        confirm_report_tx: tokio::sync::mpsc::Sender<
            crate::infrastructure::collect::command::ProcessCollectTxConfirmReportCommand,
        >,
    ) -> Self {
        let (shutdown_tx, shutdown_rx1) = tokio::sync::broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();

        let (scanner_message_tx, scanner_message_rx) = mpsc::channel(100);
        let (dispatcher_message_tx, dispatcher_message_rx) = mpsc::channel(100);
        let (intent_tx, mut intent_rx) = mpsc::channel(1000);

        // 创建Scanner Actor
        let scanner_actor = CollectorShadowScannerActor::new(
            pool.clone(),
            ScannerConfig::default(),
            intent_tx.clone(),
            shutdown_rx1,
            scanner_message_rx,
        );
        let scanner_handle = Some(tokio::spawn(async move {
            scanner_actor.run().await;
        }));

        // 创建Dispatcher Actor
        let dispatcher_actor = CollectorShadowDispatcherActor::new(
            pool.clone(),
            DispatcherConfig::default(),
            tx_tx,
            report_tx,
            confirm_report_tx,
            shutdown_rx2,
            dispatcher_message_rx,
        );
        let dispatcher_handle = Some(tokio::spawn(async move {
            dispatcher_actor.run().await;
        }));

        // 创建意图转发任务（从intent_rx接收意图，发送给Dispatcher Actor）
        let dispatcher_message_tx_clone = dispatcher_message_tx.clone();
        tokio::spawn(async move {
            while let Some(intent) = intent_rx.recv().await {
                if let Err(e) = dispatcher_message_tx_clone
                    .send(DispatcherActorMessage::HandleIntent(intent))
                    .await
                {
                    error!("Failed to send intent to Dispatcher Actor: {}", e);
                }
            }
        });

        Self {
            shutdown_tx,
            scanner_message_tx,
            dispatcher_message_tx,
            scanner_handle,
            dispatcher_handle,
            intent_tx,
        }
    }

    /// 启动Shadow系统
    pub async fn start(&self) {
        info!("Starting Collector Shadow System");

        // 启动Scanner Actor
        if let Err(e) = self.scanner_message_tx.send(ScannerActorMessage::Start).await {
            error!("Failed to start Scanner Actor: {}", e);
        }

        info!("Collector Shadow System started");
    }

    /// 停止Shadow系统
    pub async fn stop(&mut self) {
        info!("Stopping Collector Shadow System");

        // 发送停止信号
        let _ = self.shutdown_tx.send(());

        // 等待Actor结束
        if let Some(handle) = self.scanner_handle.take() {
            let _ = handle.await;
        }

        if let Some(handle) = self.dispatcher_handle.take() {
            let _ = handle.await;
        }

        info!("Collector Shadow System stopped");
    }

    /// 获取意图发送器
    pub fn get_intent_tx(&self) -> mpsc::Sender<CollectIntent> {
        self.intent_tx.clone()
    }
}
