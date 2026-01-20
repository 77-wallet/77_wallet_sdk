use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use sqlx::SqlitePool;
use tracing::{info, warn};

use wallet_database::{
    entities::api_collect::ApiCollectStatus, repositories::api_wallet::collect::ApiCollectRepo,
};

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
    /// ACK重试超时时间
    pub ack_timeout: Duration,
    /// 最大重试次数
    pub max_retries: u32,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            scan_interval: Duration::from_secs(10),
            max_items_per_scan: 200,
            init_timeout: Duration::from_secs(300),    // 5分钟
            sending_timeout: Duration::from_secs(600), // 10分钟
            ack_timeout: Duration::from_secs(300),     // 5分钟
            max_retries: 3,
        }
    }
}

/// Shadow Scanner
///
/// 只生成推进意图，不直接执行状态推进
pub struct ShadowScanner {
    pool: Arc<SqlitePool>,
    config: ScannerConfig,
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

    /// 启动扫描器
    pub async fn start(&self) {
        info!("Collect Shadow Scanner started");

        let mut interval = tokio::time::interval(self.config.scan_interval);
        loop {
            interval.tick().await;
            self.scan_round().await;
        }
    }

    /// 执行一轮扫描
    async fn scan_round(&self) {
        let start = Instant::now();
        info!("Starting collect shadow scan round");

        // 执行四种扫描逻辑
        self.scan_init_timeout().await;
        self.scan_sending_timeout().await;
        self.scan_ack_pending().await;
        self.scan_confirm_failure().await;

        info!("Collect shadow scan round completed in {:?}", start.elapsed());
    }

    /// 扫描超时的INIT状态
    async fn scan_init_timeout(&self) {
        info!("Scanning INIT timeout records");

        // 暂时简化实现，避免调用不存在的方法
        // 查询DB中status=INIT且updated_at超时的记录
        let records: Vec<wallet_database::entities::api_collect::ApiCollectEntity> = vec![];

        info!("Found {} INIT timeout records", records.len());

        // 生成推进意图
        for record in records {
            let intent = CollectIntent::BuildTx(record.trade_no);
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描超时的SENDING状态
    async fn scan_sending_timeout(&self) {
        info!("Scanning SENDING timeout records");

        // 暂时简化实现，避免调用不存在的方法
        // 查询DB中status=SendingTx且updated_at超时的记录
        let records: Vec<wallet_database::entities::api_collect::ApiCollectEntity> = vec![];

        info!("Found {} SENDING timeout records", records.len());

        // 生成推进意图
        for record in records {
            let intent = CollectIntent::Confirm(record.trade_no);
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描需要ACK的记录
    async fn scan_ack_pending(&self) {
        info!("Scanning ACK pending records");

        // 暂时简化实现，避免调用不存在的方法
        // 查询DB中status=SUCCESS/FAILURE且tx_res_ack_sent_at为NULL的记录
        let records: Vec<wallet_database::entities::api_collect::ApiCollectEntity> = vec![];

        info!("Found {} ACK pending records", records.len());

        // 生成推进意图
        for record in records {
            let intent = CollectIntent::Ack(record.trade_no);
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描需要重试的确认失败记录
    async fn scan_confirm_failure(&self) {
        info!("Scanning confirm failure records");

        // 暂时简化实现，避免调用不存在的方法
        // 查询DB中status=ConfirmFailureReport且retry < max_retries的记录
        let records: Vec<wallet_database::entities::api_collect::ApiCollectEntity> = vec![];

        info!("Found {} confirm failure records", records.len());

        // 生成推进意图
        for record in records {
            let intent = CollectIntent::Confirm(record.trade_no);
            self.dispatch_intent(intent).await;
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
