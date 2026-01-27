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

use crate::infrastructure::collect::shadow::{ChainIntent, SideEffectIntent};

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
        // 推荐顺序：按照不可逆事实时间轴
        // 1. 订单确认 ACK
        // 2. 构建交易
        // 3. 广播交易
        // 4. 上传交易执行回执
        // 5. 发送结果 ACK
        // 6. 上传服务费
        self.scan_order_ack_not_sent().await;
        self.scan_can_build().await;
        self.scan_can_broadcast().await;
        self.scan_need_tx_exec_receipt_upload().await;
        self.scan_confirmed_need_result_ack().await;
        self.scan_confirmed_need_service_fee_upload().await;

        info!("Collect shadow scan round completed in {:?}", start.elapsed());
    }

    /// 扫描“允许构建 raw_tx”的交易
    ///
    /// 事实条件（强顺序屏障）：
    /// - order_ack_sent_at IS NOT NULL   // 订单确认已完成
    /// - raw_tx IS NULL
    /// - build_blocked_at IS NULL
    ///
    /// ⚠️ 设计说明：
    /// BuildTx 必须显式依赖 OrderAck 完成，
    /// 禁止移除 order_ack_sent_at 条件，否则会破坏强顺序保证。
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
            let intent = CollectIntent::Chain(ChainIntent::BuildTx(record.trade_no));
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
            let intent = CollectIntent::Chain(ChainIntent::BroadcastTx(record.trade_no));
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描需要发送结果确认 ACK 的交易
    ///
    /// 事实条件（强顺序屏障）：
    /// - tx_exec_receipt_uploaded_at IS NOT NULL
    /// - result_ack_sent_at IS NULL
    ///
    /// ⚠️ 设计说明：
    /// ResultAck 必须发生在 TxExecReceipt 上传之后。
    /// 禁止使用 transaction_time 作为前置条件（共享前提事实）。
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
            let intent = CollectIntent::SideEffect(SideEffectIntent::SendResultAck(record.trade_no));
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描已确认但未上传服务费的交易
    ///
    /// 事实条件：
    /// - transaction_time IS NOT NULL
    /// - service_fee_uploaded_at IS NULL
    ///
    /// 对应动作：
    /// - 生成UploadServiceFee意图
    async fn scan_confirmed_need_service_fee_upload(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning confirmed need service fee upload records");

        // 查询DB中已确认但未上传服务费的记录
        let records = match wallet_database::repositories::api_wallet::collect::ApiCollectRepo::scan_confirmed_need_service_fee_upload(
            &self.pool,
            self.config.max_items_per_scan,
        ).await {
            Ok(records) => records,
            Err(e) => {
                error!(error = %e, "Failed to scan confirmed need service fee upload records");
                return;
            }
        };

        // 保存原始记录数
        let original_count = records.len();
        info!(found = %original_count, "Found confirmed need service fee upload records");

        // 生成推进意图
        for record in records {
            let intent = CollectIntent::SideEffect(SideEffectIntent::UploadServiceFee(record.trade_no));
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描需要上传交易执行回执的交易
    ///
    /// 事实条件：
    /// - transaction_time IS NOT NULL
    /// - tx_exec_receipt_uploaded_at IS NULL
    ///
    /// 对应动作：
    /// - 生成UploadTxExecReceipt意图
    async fn scan_need_tx_exec_receipt_upload(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning need tx exec receipt upload records");

        // 查询DB中需要上传交易执行回执的记录
        let records = match wallet_database::repositories::api_wallet::collect::ApiCollectRepo::scan_need_tx_exec_receipt_upload(
            &self.pool,
            self.config.max_items_per_scan,
        ).await {
            Ok(records) => records,
            Err(e) => {
                error!(error = %e, "Failed to scan need tx exec receipt upload records");
                return;
            }
        };

        // 保存原始记录数
        let original_count = records.len();
        info!(found = %original_count, "Found need tx exec receipt upload records");

        // 生成推进意图
        for record in records {
            // 日志中区分首次尝试和重试
            if record.tx_exec_receipt_attempted_at.is_some() {
                info!(trade_no = %record.trade_no, "Retrying tx exec receipt upload");
            } else {
                info!(trade_no = %record.trade_no, "First attempt tx exec receipt upload");
            }
            let intent = CollectIntent::SideEffect(SideEffectIntent::UploadTxExecReceipt(record.trade_no));
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描需要发送订单确认 ACK 的交易
    ///
    /// 事实条件：
    /// - order_ack_sent_at IS NULL
    ///
    /// 对应动作：
    /// - 生成SendOrderAck意图
    ///
    /// ⚠️ 只看推进事实，不看行为事实：
    /// - order_ack_sent_at IS NULL：尚未发送订单确认（推进事实）
    ///
    /// ❌ 不检查 order_ack_attempted_at（这是行为事实，不参与判断）
    async fn scan_order_ack_not_sent(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning order ack not sent records");

        // 查询DB中需要发送订单确认 ACK 的记录
        let records = match wallet_database::repositories::api_wallet::collect::ApiCollectRepo::scan_need_order_ack(
            &self.pool,
            self.config.max_items_per_scan,
        ).await {
            Ok(records) => records,
            Err(e) => {
                error!(error = %e, "Failed to scan order ack not sent records");
                return;
            }
        };

        // 保存原始记录数
        let original_count = records.len();
        info!(found = %original_count, "Found order ack not sent records");

        // 生成推进意图
        for record in records {
            // 日志中区分首次尝试和重试
            if record.order_ack_attempted_at.is_some() {
                info!(trade_no = %record.trade_no, "Retrying order ack send");
            } else {
                info!(trade_no = %record.trade_no, "First attempt order ack send");
            }
            let intent = CollectIntent::SideEffect(SideEffectIntent::SendOrderAck(record.trade_no));
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
