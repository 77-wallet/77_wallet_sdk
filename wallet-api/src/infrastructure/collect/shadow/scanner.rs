// collect/shadow/scanner.rs
//
// 重要设计原则：
// 1. Scanner 只看事实字段，永远不看 status
// 2. 事实字段包括：raw_tx、transaction_time、finished_at、order_ack_sent_at、result_ack_sent_at
// 3. 所有状态推进都基于事实，而非状态机
//
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use sqlx::SqlitePool;
use tracing::{error, info, warn};

use super::CollectIntent;

/// Shadow Scanner 配置
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// 扫描间隔
    pub scan_interval: Duration,
    /// 每轮最大处理数量
    pub max_items_per_scan: usize,
    /// INIT状态超时时间
    pub init_timeout: Duration,
    /// SENDING状态超时时间
    pub sending_timeout: Duration,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            scan_interval: Duration::from_secs(10),
            max_items_per_scan: 200,
            init_timeout: Duration::from_secs(300),    // 5分钟
            sending_timeout: Duration::from_secs(600), // 10分钟
        }
    }
}

/// Shadow Scanner
///
/// 只生成推进意图，不直接执行状态推进
pub struct ShadowScanner {
    pool: Arc<SqlitePool>,
    /// Scanner配置
    pub config: ScannerConfig,
    intent_tx: tokio::sync::mpsc::Sender<CollectIntent>,
}

impl ShadowScanner {
    pub fn new(
        pool: Arc<SqlitePool>,
        config: ScannerConfig,
        intent_tx: tokio::sync::mpsc::Sender<CollectIntent>,
    ) -> Self {
        Self { pool, config, intent_tx }
    }

    /// 执行一轮扫描
    pub async fn scan_round(&self) {
        let start = Instant::now();
        info!("Starting collect shadow scan round");

        // 执行四种扫描逻辑：基于事实驱动
        self.scan_can_build().await;
        self.scan_can_broadcast().await;
        self.scan_confirmed_done().await;
        self.scan_confirmed_done_without_ack().await;

        info!("Collect shadow scan round completed in {:?}", start.elapsed());
    }

    /// 扫描可构建的交易：raw_tx为空且building_at为空或已超时
    async fn scan_can_build(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning can build records");

        // 查询DB中可构建的记录
        let records = match wallet_database::repositories::api_wallet::collect::ApiCollectRepo::scan_can_build(
            &self.pool,
            self.config.max_items_per_scan,
        ).await {
            Ok(records) => records,
            Err(e) => {
                error!(error = %e, "Failed to scan can build records");
                return;
            }
        };

        // 保存原始记录数
        let original_count = records.len();
        info!(found = %original_count, "Found can build records");

        // 生成推进意图
        for record in records {
            let intent = CollectIntent::BuildTx(record.trade_no);
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描可广播的交易：raw_tx存在且transaction_time为空且last_broadcast_at为空或已超时
    async fn scan_can_broadcast(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning can broadcast records");

        // 查询DB中可广播的记录
        let records = match wallet_database::repositories::api_wallet::collect::ApiCollectRepo::scan_can_broadcast(
            &self.pool,
            self.config.max_items_per_scan,
        ).await {
            Ok(records) => records,
            Err(e) => {
                error!(error = %e, "Failed to scan can broadcast records");
                return;
            }
        };

        // 保存原始记录数
        let original_count = records.len();
        info!(found = %original_count, "Found can broadcast records");

        // 生成推进意图
        for record in records {
            let intent = CollectIntent::Broadcast(record.trade_no);
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描已确认但未完成的交易：transaction_time存在且finished_at为空
    async fn scan_confirmed_done(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning confirmed done records");

        // 查询DB中已确认但未完成的记录
        let records = match wallet_database::repositories::api_wallet::collect::ApiCollectRepo::scan_confirmed_done(
            &self.pool,
            self.config.max_items_per_scan,
        ).await {
            Ok(records) => records,
            Err(e) => {
                error!(error = %e, "Failed to scan confirmed done records");
                return;
            }
        };

        // 保存原始记录数
        let original_count = records.len();
        info!(found = %original_count, "Found confirmed done records");

        // 生成推进意图
        for record in records {
            // 这里暂时没有对应的意图，因为 confirm 不由 Shadow Worker 处理
            // 链上结果由 MQTT 注入，由 Domain 层落库
            info!(trade_no = %record.trade_no, "Confirmed done record found, will be handled by chain callback");
        }
    }

    /// 扫描已确认但未发送TxRes ACK的交易
    async fn scan_confirmed_done_without_ack(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning confirmed done without ACK records");

        // 查询DB中已确认但未发送TxRes ACK的记录
        let records = match wallet_database::repositories::api_wallet::collect::ApiCollectRepo::scan_confirmed_done_without_ack(
            &self.pool,
            self.config.max_items_per_scan,
        ).await {
            Ok(records) => records,
            Err(e) => {
                error!(error = %e, "Failed to scan confirmed done without ACK records");
                return;
            }
        };

        // 保存原始记录数
        let original_count = records.len();
        info!(found = %original_count, "Found confirmed done without ACK records");

        // 处理每个记录，发送ACK
        for record in records {
            let trade_no = record.trade_no.clone();
            info!(trade_no = %trade_no, "Processing confirmed done without ACK record");

            // 获取backend_api
            let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

            // 发送TxRes ACK
            match backend_api
                .trans_event_ack(&wallet_transport_backend::request::api_wallet::transaction::TransEventAckReq::new(
                    &trade_no,
                    wallet_transport_backend::request::api_wallet::transaction::TransType::Col,
                    wallet_transport_backend::request::api_wallet::transaction::TransAckType::TxRes,
                ))
                .await
            {
                Ok(_) => {
                    info!(trade_no = %trade_no, "TxRes ACK sent successfully");
                    // 标记ACK发送，并设置终态
                    if let Err(e) = wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_result_ack_sent(
                        &self.pool,
                        &trade_no,
                    ).await {
                        error!(trade_no = %trade_no, error = %e, "Failed to mark result ACK sent");
                    }
                },
                Err(e) => {
                    error!(trade_no = %trade_no, error = %e, "Failed to send TxRes ACK");
                    // 标记ACK发送，但不设置终态，允许重试
                    if let Err(e) = wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_result_ack_sent(
                        &self.pool,
                        &trade_no,
                    ).await {
                        error!(trade_no = %trade_no, error = %e, "Failed to mark result ACK sent");
                    }
                },
            }
        }
    }

    /// 分发推进意图
    async fn dispatch_intent(&self, intent: CollectIntent) {
        info!(?intent, "Generated collect intent");

        // 将意图发送给Dispatcher
        if let Err(e) = self.intent_tx.send(intent).await {
            warn!("Failed to send collect intent: {}", e);
        }
    }
}
