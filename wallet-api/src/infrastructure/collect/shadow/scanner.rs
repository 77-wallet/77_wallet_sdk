// collect/shadow/scanner.rs
//
// Scanner 设计铁律：
//
// 1. Scanner 只读取“不可逆事实字段”，不读取、不推断、不解释 status
// 2. Scanner 不使用时间字段做任何决策（building_at / last_broadcast_at 仅用于观测）
// 3. Scanner 不判断“该不该做”，只判断“是否满足事实条件”
// 4. Scanner 的唯一职责：
//    事实快照 -> 生成 CollectIntent
// 5. Scanner 中的方法命名必须是事实条件的直接翻译，禁止使用状态语义词（done / finished / completed）
//
use std::time::{Duration, Instant};

use tracing::{error, info, warn};
use wallet_database::CollectDbPool;

use super::CollectIntent;

/// Shadow Scanner 配置
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// 扫描间隔
    pub scan_interval: Duration,
    /// 每轮最大处理数量
    pub max_items_per_scan: usize,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self { scan_interval: Duration::from_secs(10), max_items_per_scan: 200 }
    }
}

/// Shadow Scanner
///
///
/// 只生成推进意图，不直接执行状态推进
pub struct ShadowScanner {
    pool: CollectDbPool,
    /// Scanner配置
    pub config: ScannerConfig,
    intent_tx: tokio::sync::mpsc::Sender<CollectIntent>,
}

impl ShadowScanner {
    pub fn new(
        pool: CollectDbPool,
        config: ScannerConfig,
        intent_tx: tokio::sync::mpsc::Sender<CollectIntent>,
    ) -> Self {
        Self { pool, config, intent_tx }
    }

    /// 执行一轮扫描
    pub async fn scan_round(&self) {
        let start = Instant::now();
        info!("Starting collect shadow scan round");

        // 执行扫描逻辑：基于事实驱动
        self.scan_can_build().await;
        self.scan_can_broadcast().await;
        self.scan_confirmed_need_result_ack().await;

        info!("Collect shadow scan round completed in {:?}", start.elapsed());
    }

    /// 扫描“允许构建 raw_tx”的交易
    ///
    /// 事实条件：
    /// - raw_tx IS NULL
    /// - build_blocked_at IS NULL
    ///
    /// ⚠️ Scanner 不关心：
    /// - 为什么不能构建
    /// - 之前是否构建失败
    /// - 是否超时
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

    /// 扫描“允许广播”的交易
    ///
    /// 事实条件：
    /// - raw_tx IS NOT NULL
    /// - transaction_time IS NULL
    ///
    /// ⚠️ last_broadcast_at 仅用于观测，不参与决策
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

    /// 扫描已确认但未发送TxRes ACK的交易
    ///
    /// 事实条件：
    /// - transaction_time IS NOT NULL
    /// - finished_at IS NULL
    /// - result_ack_sent_at IS NULL
    ///
    /// 对应动作：
    /// - 生成SendResultAck意图
    async fn scan_confirmed_need_result_ack(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning confirmed need result ACK records");

        // 查询DB中已确认但未发送TxRes ACK的记录
        let records = match wallet_database::repositories::api_wallet::collect::ApiCollectRepo::scan_confirmed_need_result_ack(
            &self.pool,
            self.config.max_items_per_scan,
        ).await {
            Ok(records) => records,
            Err(e) => {
                error!(error = %e, "Failed to scan confirmed need result ACK records");
                return;
            }
        };

        // 保存原始记录数
        let original_count = records.len();
        info!(found = %original_count, "Found confirmed need result ACK records");

        // 生成推进意图
        for record in records {
            let intent = CollectIntent::SendResultAck(record.trade_no);
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
