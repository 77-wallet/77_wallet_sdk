// collect_fee/shadow/scanner.rs
//
// Scanner 设计铁律：
//
// 1. Scanner 只读取“不可逆事实字段”，不读取、不推断、不解释 status
// 2. Scanner 不使用时间字段做任何决策（building_at / last_broadcast_at 仅用于观测）
// 3. Scanner 不判断“该不该做”，只判断“是否满足事实条件”
// 4. Scanner 的唯一职责：
//    事实快照 -> 生成 FeeIntent
// 5. Scanner 中的方法命名必须是事实条件的直接翻译，禁止使用状态语义词（done / finished / completed）
//
use std::time::{Duration, Instant};

use tracing::{error, info, warn};
use wallet_database::{CollectDbPool, entities::api_fee::ApiFeeEntity};

use crate::infrastructure::collect_fee::shadow::{FeeChainIntent, FeeSideEffectIntent};

use super::FeeIntent;

/// ============================================================================
///                            共用 Predicate 函数
/// ============================================================================
///
/// 注意：所有 predicate 函数必须是纯函数，不得：
/// - 写 DB
/// - 发请求
/// - 依赖时间
/// - 依赖外部状态
/// ============================================================================

/// 链推进类（Chain Progress）predicate
/// ----------------------------------------------------------------------------

/// 检查是否可以构建交易
///
/// 事实条件（强顺序屏障）：
/// - tx_ack_sent_at IS NOT NULL   // 订单确认已完成
/// - raw_tx IS NULL
fn can_build(fee: &ApiFeeEntity) -> bool {
    fee.tx_ack_sent_at.is_some() && fee.raw_tx.is_empty()
}

/// 检查是否可以广播交易
///
/// 事实条件：
/// - raw_tx IS NOT NULL
/// - transaction_time IS NULL
fn can_broadcast(fee: &ApiFeeEntity) -> bool {
    !fee.raw_tx.is_empty() && fee.transaction_time.is_none()
}

/// 副作用类（Side Effect）predicate
/// ----------------------------------------------------------------------------

/// 检查是否需要发送交易 ACK
///
/// 事实条件：
/// - tx_ack_sent_at IS NULL
fn need_tx_ack(fee: &ApiFeeEntity) -> bool {
    fee.tx_ack_sent_at.is_none()
}

/// 检查是否需要上传交易执行回执
///
/// 事实条件：
/// - transaction_time IS NOT NULL
/// - tx_exec_receipt_uploaded_at IS NULL
fn need_tx_exec_receipt_upload(fee: &ApiFeeEntity) -> bool {
    fee.transaction_time.is_some() && fee.tx_exec_receipt_uploaded_at.is_none()
}

/// 检查是否需要发送交易结果 ACK
///
/// 事实条件：
/// - tx_exec_receipt_uploaded_at IS NOT NULL   // 交易执行回执上传已完成
/// - tx_res_ack_sent_at IS NULL
fn need_tx_res_ack(fee: &ApiFeeEntity) -> bool {
    fee.tx_exec_receipt_uploaded_at.is_some() && fee.tx_res_ack_sent_at.is_none()
}

/// 终态 / 完成判断（Future Use）
/// ----------------------------------------------------------------------------

/// 检查交易是否已完成所有链事实
///
/// 事实条件：
/// - transaction_time IS NOT NULL
fn is_chain_finished(fee: &ApiFeeEntity) -> bool {
    fee.transaction_time.is_some()
}

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
    intent_tx: tokio::sync::mpsc::Sender<FeeIntent>,
}

impl ShadowScanner {
    pub fn new(
        pool: CollectDbPool,
        config: ScannerConfig,
        intent_tx: tokio::sync::mpsc::Sender<FeeIntent>,
    ) -> Self {
        Self { pool, config, intent_tx }
    }

    /// 执行一轮扫描
    pub async fn scan_round(&self) {
        let start = Instant::now();
        info!("Starting fee shadow scan round");

        // 获取一次快照
        let snapshot = self.get_fee_records().await;

        // 执行扫描逻辑：基于事实驱动
        // 推荐顺序：按照不可逆事实时间轴
        // 1. 交易确认 ACK
        // 2. 构建交易
        // 3. 广播交易
        // 4. 上传交易执行回执
        // 5. 发送交易结果 ACK
        self.scan_tx_ack_not_sent(&snapshot).await;
        self.scan_can_build(&snapshot).await;
        self.scan_can_broadcast(&snapshot).await;
        self.scan_need_tx_exec_receipt_upload(&snapshot).await;
        self.scan_confirmed_need_tx_res_ack(&snapshot).await;

        info!("Fee shadow scan round completed in {:?}", start.elapsed());
    }

    /// 扫描需要发送交易确认 ACK 的记录
    ///
    /// 事实条件：
    /// - tx_ack_sent_at IS NULL
    ///
    /// 对应动作：
    /// - 生成SendTxAck意图
    ///
    /// ⚠️ 只看推进事实，不看行为事实：
    /// - tx_ack_sent_at IS NULL：尚未发送交易确认（推进事实）
    async fn scan_tx_ack_not_sent(&self, snapshot: &[ApiFeeEntity]) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning tx ack not sent records");

        // 过滤出需要发送交易确认 ACK 的记录
        let filtered_records: Vec<&ApiFeeEntity> = snapshot.into_iter().filter(|fee| need_tx_ack(fee)).collect();

        // 保存原始记录数
        let original_count = filtered_records.len();
        info!(found = %original_count, "Found tx ack not sent records");

        // 生成推进意图
        for record in filtered_records {
            let intent = FeeIntent::SideEffect(FeeSideEffectIntent::SendTxAck(record.trade_no.clone()));
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描“允许构建 raw_tx”的记录
    ///
    /// 事实条件（强顺序屏障）：
    /// - tx_ack_sent_at IS NOT NULL   // 订单确认已完成
    /// - raw_tx IS NULL
    ///
    /// ⚠️ 设计说明：
    /// BuildTx 必须显式依赖 TxAck 完成，
    /// 禁止移除 tx_ack_sent_at 条件，否则会破坏强顺序保证。
    ///
    /// ⚠️ Scanner 不关心：
    /// - 为什么不能构建
    /// - 之前是否构建失败
    /// - 是否超时
    async fn scan_can_build(&self, snapshot: &[ApiFeeEntity]) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning can build records");

        // 过滤出可构建的记录
        let filtered_records: Vec<&ApiFeeEntity> = snapshot.into_iter().filter(|fee| can_build(fee)).collect();

        // 保存原始记录数
        let original_count = filtered_records.len();
        info!(found = %original_count, "Found can build records");

        // 生成推进意图
        for record in filtered_records {
            let intent = FeeIntent::Chain(FeeChainIntent::BuildTx(record.trade_no.clone()));
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描“允许广播”的记录
    ///
    /// 事实条件：
    /// - raw_tx IS NOT NULL
    /// - transaction_time IS NULL
    async fn scan_can_broadcast(&self, snapshot: &[ApiFeeEntity]) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning can broadcast records");

        // 过滤出可广播的记录
        let filtered_records: Vec<&ApiFeeEntity> =
            snapshot.into_iter().filter(|fee| can_broadcast(fee)).collect();

        // 保存原始记录数
        let original_count = filtered_records.len();
        info!(found = %original_count, "Found can broadcast records");

        // 生成推进意图
        for record in filtered_records {
            let intent = FeeIntent::Chain(FeeChainIntent::BroadcastTx(record.trade_no.clone()));
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描已确认但未发送TxRes ACK的记录
    ///
    /// 事实条件（强顺序屏障）：
    /// - transaction_time IS NOT NULL
    /// - tx_res_ack_sent_at IS NULL
    ///
    /// ⚠️ 设计说明：
    /// TxResAck 必须发生在交易确认之后。
    ///
    /// 对应动作：
    /// - 生成SendTxResAck意图
    async fn scan_confirmed_need_tx_res_ack(&self, snapshot: &[ApiFeeEntity]) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning confirmed need tx res ACK records");

        // 过滤出已确认但未发送TxRes ACK的记录
        let filtered_records: Vec<&ApiFeeEntity> =
            snapshot.into_iter().filter(|fee| need_tx_res_ack(fee)).collect();

        // 保存原始记录数
        let original_count = filtered_records.len();
        info!(found = %original_count, "Found confirmed need tx res ACK records");

        // 生成推进意图
        for record in filtered_records {
            let intent = FeeIntent::SideEffect(FeeSideEffectIntent::SendTxResAck(record.trade_no.clone()));
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描需要上传交易执行回执的记录
    ///
    /// 事实条件：
    /// - transaction_time IS NOT NULL
    ///
    /// 对应动作：
    /// - 生成UploadTxExecReceipt意图
    async fn scan_need_tx_exec_receipt_upload(&self, snapshot: &[ApiFeeEntity]) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning need tx exec receipt upload records");

        // 过滤出需要上传交易执行回执的记录
        let filtered_records: Vec<&ApiFeeEntity> =
            snapshot.into_iter().filter(|fee| need_tx_exec_receipt_upload(fee)).collect();

        // 保存原始记录数
        let original_count = filtered_records.len();
        info!(found = %original_count, "Found need tx exec receipt upload records");

        // 生成推进意图
        for record in filtered_records {
            let intent =
                FeeIntent::SideEffect(FeeSideEffectIntent::UploadTxExecReceipt(record.trade_no.clone()));
            self.dispatch_intent(intent).await;
        }
    }

    /// 分发推进意图
    async fn dispatch_intent(&self, intent: FeeIntent) {
        info!(?intent, "Generated fee intent");

        // 将意图发送给Dispatcher
        if let Err(e) = self.intent_tx.send(intent).await {
            warn!("Failed to send fee intent: {}", e);
        }
    }

    /// 尝试基于当前事实推进一个阶段
    ///
    /// 注意：try_advance 每次最多推进一个阶段
    /// 多阶段推进依赖后续 Tick 或定时扫描
    ///
    /// 参数：
    /// - trade_no: 手续费交易编号
    ///
    /// 行为：
    /// 1. 查询最新的DB状态
    /// 2. 基于事实状态，按照优先级顺序检查可推进点
    /// 3. 找到第一个满足条件的推进点，生成对应意图
    /// 4. 发送意图并返回
    pub async fn try_advance(&self, trade_no: &str) {
        info!(trade_no = %trade_no, "Try advancing fee transaction");

        // 查询最新的DB状态
        // 注意：这里需要实现对应的查询方法，目前使用模拟数据
        let fee = self.get_fee_by_trade_no(trade_no).await;

        if let Some(fee) = fee {
            // 按照优先级顺序检查可推进点
            // 1. 检查是否需要发送交易 ACK
            if need_tx_ack(&fee) {
                info!(trade_no = %trade_no, "Need to send tx ACK");
                let intent =
                    FeeIntent::SideEffect(FeeSideEffectIntent::SendTxAck(trade_no.to_string()));
                self.dispatch_intent(intent).await;
                return;
            }

            // 2. 检查是否可以构建交易
            // 注意：BuildTx 必须显式依赖 TxAck 完成
            if can_build(&fee) {
                info!(trade_no = %trade_no, "Can build transaction");
                let intent = FeeIntent::Chain(FeeChainIntent::BuildTx(trade_no.to_string()));
                self.dispatch_intent(intent).await;
                return;
            }

            // 3. 检查是否可以广播交易
            if can_broadcast(&fee) {
                info!(trade_no = %trade_no, "Can broadcast transaction");
                let intent = FeeIntent::Chain(FeeChainIntent::BroadcastTx(trade_no.to_string()));
                self.dispatch_intent(intent).await;
                return;
            }

            // 4. 检查是否需要上传交易执行回执
            if need_tx_exec_receipt_upload(&fee) {
                info!(trade_no = %trade_no, "Need to upload tx exec receipt");
                let intent = FeeIntent::SideEffect(FeeSideEffectIntent::UploadTxExecReceipt(
                    trade_no.to_string(),
                ));
                self.dispatch_intent(intent).await;
                return;
            }

            // 5. 检查是否需要发送交易结果 ACK
            if need_tx_res_ack(&fee) {
                info!(trade_no = %trade_no, "Need to send tx res ACK");
                let intent =
                    FeeIntent::SideEffect(FeeSideEffectIntent::SendTxResAck(trade_no.to_string()));
                self.dispatch_intent(intent).await;
                return;
            }

            // 无可用推进点
            info!(trade_no = %trade_no, "No advancement possible based on current facts");
        } else {
            error!(trade_no = %trade_no, "Fee record not found");
        }
    }

    /// 获取手续费记录（模拟数据）
    async fn get_fee_records(&self) -> Vec<ApiFeeEntity> {
        // 注意：这里需要实现对应的查询方法，目前返回空数组
        // 实际实现中应该调用 ApiFeeRepo 的方法查询数据库
        Vec::new()
    }

    /// 根据交易编号获取手续费记录（模拟数据）
    async fn get_fee_by_trade_no(&self, trade_no: &str) -> Option<ApiFeeEntity> {
        // 注意：这里需要实现对应的查询方法，目前返回 None
        // 实际实现中应该调用 ApiFeeRepo 的方法查询数据库
        None
    }
}
