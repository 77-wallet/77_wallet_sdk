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
// IMPORTANT:
// All ApiFeeRepo::scan_xxx SQL conditions MUST be equivalent
// to the corresponding predicate function in this file.
// This ensures that scanner, try_advance, and future components
// all use the same logic and do not diverge.
//
// IMPORTANT DESIGN NOTE:
//
// build_blocked_at is a system-level backpressure mechanism.
// It MUST NOT be removed or replaced by execution-time checks (e.g. check_fee).
//
// - check_fee: answers "can we build NOW?"
// - build_blocked_at: answers "should the system keep trying to build?"
//
// build_blocked_at represents a FACT, not a retry hint.
// It affects scanner predicates across scan rounds.
// Execution-time guards (e.g. check_fee) MUST NOT replace it.
//
// Reordering execution logic does NOT eliminate the necessity of build_blocked_at.
//
// ❌ WRONG EXAMPLE:
//
// if !check_fee() {
//     return Ok(());
// }
//
// This is NOT sufficient to replace build_blocked_at.
// The scanner will continue to emit BuildTx intents.
//

/// ============================================================================
/// 手续费（Service Fee）流程铁律（必须遵守）
/// ============================================================================
///
/// 【核心定位】
/// 手续费流程是「构建阶段的失败分支」，而不是一条独立的成功路径。
///
/// 一旦确认“手续费不足”，该交易在**业务语义上已经结束**，
/// 后续只允许做“结果上报型副作用”，禁止任何继续推进链上流程。
///
/// ---------------------------------------------------------------------------
/// 一、手续费不足的定义（事实，而非状态）
/// ---------------------------------------------------------------------------
/// 当且仅当满足以下事实条件时，视为手续费不足：
///
/// - build_blocked_at IS NOT NULL
/// - need_service_fee = true
///
/// ⚠️ 注意：
/// - 手续费不足 ≠ 链上失败
/// - 手续费不足发生在【构建阶段】
/// - 与 tx_hash / transaction_time 无关
///
/// ---------------------------------------------------------------------------
/// 二、手续费不足的处理铁律（不可破坏）
/// ---------------------------------------------------------------------------
///
/// 一旦确认手续费不足：
///
/// 1. 该交易【不再进入广播阶段】
/// 2. 该交易【不会产生 tx_hash】
/// 3. 该交易【不会发生 transaction_time】
/// 4. 该交易【不会进入重试 / 打手续费流程】
///
/// 允许的唯一推进方向：
///
/// - 视为“构建失败的最终结果”
/// - 直接进入 tx_exec_receipt_upload
/// - 上报失败执行结果给后端
///
/// ---------------------------------------------------------------------------
/// 三、tx_exec_receipt_upload 在手续费场景下的语义
/// ---------------------------------------------------------------------------
///
/// tx_exec_receipt_upload 在此场景下表示：
///
/// “我已发起过链上执行请求的**意图**，
/// 但由于手续费不足，实际未发生链上执行。”
///
/// 因此：
/// - receipt 内容为失败结果
/// - 不要求 tx_hash
/// - 不依赖 transaction_time
///
/// ---------------------------------------------------------------------------
/// 四、Scanner 约束（非常重要）
/// ---------------------------------------------------------------------------
///
/// Scanner 必须遵守以下规则：
///
/// - 不得因为手续费不足而触发：
///   - scan_can_broadcast
///   - scan_need_tx_res_ack（成功 ACK）
///
/// - 只允许触发：
///   - scan_need_tx_exec_receipt_upload（失败回执）
///
/// - tx_exec_receipt_uploaded_at 写入后：
///   - 该交易流程视为结束
///   - 不得再被任何 Scanner predicate 命中
///
/// ---------------------------------------------------------------------------
/// 五、与归集流程的关系
/// ---------------------------------------------------------------------------
///
/// 手续费流程与归集流程遵循相同的事实驱动铁律，
/// 区别仅在于：
///
/// - 手续费流程步骤更少
/// - 不存在“补打手续费后继续推进”的路径
///
/// 一旦手续费不足：
/// - 归集流程：结束
/// - 手续费流程：结束
///
/// 不允许“修复后重试”的隐式语义。
///
/// ---------------------------------------------------------------------------
/// 六、架构原则总结（一句话）
/// ---------------------------------------------------------------------------
///
/// 手续费不足不是“需要补救的异常”，
/// 而是“可以被确认并上报的最终事实”。
///
/// ============================================================================
/// END
/// ============================================================================

use std::time::{Duration, Instant};

use tracing::{error, info, warn};
use wallet_database::{CollectDbPool, entities::api_fee::ApiFeeEntity};

use crate::infrastructure::collect_fee::shadow::{FeeChainIntent, FeeSideEffectIntent};

use super::FeeIntent;

/// 导入 ApiFeeRepo
use wallet_database::repositories::api_wallet::fee::ApiFeeRepo;

/// ============================================================================
///                            推进点枚举与共用 Predicate 函数
/// ============================================================================
///
/// 推进点枚举：统一 scan_round 和 try_advance 的顺序定义
/// - 顺序只定义一次，确保 scan_round 和 try_advance 使用相同的优先级
/// - 将来添加新阶段时，只需修改此枚举，不会遗漏任何一处
/// ============================================================================
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvancementPoint {
    /// 需要发送交易确认 ACK
    NeedTxAck,
    /// 可以构建交易
    CanBuild,
    /// 可以广播交易
    CanBroadcast,
    /// 需要上传交易执行回执
    NeedTxExecReceiptUpload,
    /// 需要发送交易结果 ACK
    NeedTxResAck,
}

/// 推进点顺序常量
/// - 顺序只定义一次，确保 scan_round 和 try_advance 使用相同的优先级
/// - 将来添加新阶段时，只需修改此常量，不会遗漏任何一处
pub const ADVANCEMENT_ORDER: &[AdvancementPoint] = &[
    AdvancementPoint::NeedTxAck,
    AdvancementPoint::CanBuild,
    AdvancementPoint::CanBroadcast,
    AdvancementPoint::NeedTxExecReceiptUpload,
    AdvancementPoint::NeedTxResAck,
];

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
///
/// 注意：
/// - fee 域不使用 build_blocked_at 作为系统级背压
/// - build_blocked_at 是 collect 域的系统级背压机制
/// - fee 作为“产出手续费条件”的模块，不应被自身阻断
fn can_build(fee: &ApiFeeEntity) -> bool {
    fee.tx_ack_sent_at.is_some() && fee.raw_tx.is_none()
}

/// 检查是否可以广播交易
///
/// 事实条件：
/// - raw_tx IS NOT NULL
/// - last_broadcast_at IS NULL
/// - finished_at IS NULL
fn can_broadcast(fee: &ApiFeeEntity) -> bool {
    fee.raw_tx.is_some() && fee.last_broadcast_at.is_none() && fee.finished_at.is_none()
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
/// - last_broadcast_at IS NOT NULL
/// - tx_exec_receipt_uploaded_at IS NULL
fn need_tx_exec_receipt_upload(fee: &ApiFeeEntity) -> bool {
    fee.last_broadcast_at.is_some() && fee.tx_exec_receipt_uploaded_at.is_none()
}

/// 检查是否需要发送交易结果 ACK
///
/// 事实条件：
/// - transaction_time IS NOT NULL
/// - tx_res_ack_sent_at IS NULL
/// - finished_at IS NULL
fn need_tx_res_ack(fee: &ApiFeeEntity) -> bool {
    fee.transaction_time.is_some() && fee.tx_res_ack_sent_at.is_none() && fee.finished_at.is_none()
}

/// 终态 / 完成判断（Future Use）
/// ----------------------------------------------------------------------------

/// 检查交易是否已完成所有链事实
///
/// 事实条件：
/// - transaction_time IS NOT NULL
///
/// ⚠️ 注意：
/// - chain finished ≠ system finished
/// - 不得用于判断 Scanner 是否停止
/// - 仅表示链上结果已确定，不表示所有副作用已完成
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
///
/// NOTE:
/// Scanner and try_advance may generate duplicate intents.
/// Deduplication and idempotency MUST be guaranteed by Dispatcher / Worker.
/// This is a design choice to ensure simplicity and reliability in the scanner itself.
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

        // 执行扫描逻辑：基于事实驱动
        // 推荐顺序：按照不可逆事实时间轴
        // 1. 交易确认 ACK
        // 2. 构建交易
        // 3. 广播交易
        // 4. 上传交易执行回执
        // 5. 发送交易结果 ACK
        self.scan_need_tx_ack().await;
        self.scan_can_build().await;
        self.scan_can_broadcast().await;
        self.scan_need_tx_exec_receipt_upload().await;
        self.scan_confirmed_need_tx_res_ack().await;

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
    ///
    /// ❌ 不检查 tx_ack_attempted_at（这是行为事实，不参与判断）
    ///
    /// SQL must be equivalent to need_tx_ack()
    async fn scan_need_tx_ack(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning tx ack not sent records");

        // 查询DB中需要发送交易确认 ACK 的记录
        let records = match ApiFeeRepo::scan_need_tx_ack(
            &self.pool,
            self.config.max_items_per_scan,
        ).await {
            Ok(records) => records,
            Err(e) => {
                error!(error = %e, "Failed to scan need tx ack records");
                return;
            }
        };

        // 保存原始记录数
        let original_count = records.len();
        info!(found = %original_count, "Found tx ack not sent records");

        // 生成推进意图
        for record in records {
            // 日志中区分首次尝试和重试
            if record.tx_ack_attempted_at.is_some() {
                info!(trade_no = %record.trade_no, "Retrying tx ack send");
            } else {
                info!(trade_no = %record.trade_no, "First attempt tx ack send");
            }
            let intent = FeeIntent::SideEffect(FeeSideEffectIntent::SendTxAck(record.trade_no));
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
    /// ⚠️ 重要：
    /// - fee 域不使用 build_blocked_at 作为系统级背压
    /// - build_blocked_at 是 collect 域的系统级背压机制
    /// - fee 作为“产出手续费条件”的模块，不应被自身阻断
    ///
    /// ⚠️ Scanner 不关心：
    /// - 为什么不能构建
    /// - 之前是否构建失败
    /// - 是否超时
    ///
    /// SQL must be equivalent to can_build()
    async fn scan_can_build(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning can build records");

        // 查询DB中可构建的记录
        let records = match ApiFeeRepo::scan_can_build(
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
            let intent = FeeIntent::Chain(FeeChainIntent::BuildTx(record.trade_no));
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描“允许广播”的记录
    ///
    /// 事实条件：
    /// - raw_tx IS NOT NULL
    /// - last_broadcast_at IS NULL
    /// - finished_at IS NULL
    ///
    /// SQL must be equivalent to can_broadcast()
    async fn scan_can_broadcast(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning can broadcast records");

        // 查询DB中可广播的记录
        let records = match ApiFeeRepo::scan_can_broadcast(
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
            let intent = FeeIntent::Chain(FeeChainIntent::BroadcastTx(record.trade_no));
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描已确认但未发送TxRes ACK的记录
    ///
    /// 事实条件（强顺序屏障）：
    /// - transaction_time IS NOT NULL
    /// - tx_res_ack_sent_at IS NULL
    /// - finished_at IS NULL
    ///
    /// ⚠️ 设计说明：
    /// TxResAck 的唯一前提是“链上结果已确定”。
    /// 禁止前置条件：
    /// - 不检查 last_broadcast_at
    /// - 不检查 tx_exec_receipt_uploaded_at
    ///
    /// 对应动作：
    /// - 生成SendTxResAck意图
    ///
    /// SQL must be equivalent to need_tx_res_ack()
    async fn scan_confirmed_need_tx_res_ack(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning confirmed need tx res ACK records");

        // 查询DB中已确认但未发送TxRes ACK的记录
        let records = match ApiFeeRepo::scan_confirmed_need_tx_res_ack(
            &self.pool,
            self.config.max_items_per_scan,
        ).await {
            Ok(records) => records,
            Err(e) => {
                error!(error = %e, "Failed to scan confirmed need tx res ACK records");
                return;
            }
        };

        // 保存原始记录数
        let original_count = records.len();
        info!(found = %original_count, "Found confirmed need tx res ACK records");

        // 生成推进意图
        for record in records {
            let intent = FeeIntent::SideEffect(FeeSideEffectIntent::SendTxResAck(record.trade_no));
            self.dispatch_intent(intent).await;
        }
    }

    /// 扫描需要上传交易执行回执的记录
    ///
    /// 事实条件：
    /// - last_broadcast_at IS NOT NULL
    /// - tx_exec_receipt_uploaded_at IS NULL
    ///
    /// 对应动作：
    /// - 生成UploadTxExecReceipt意图
    ///
    /// SQL must be equivalent to need_tx_exec_receipt_upload()
    async fn scan_need_tx_exec_receipt_upload(&self) {
        info!(max_items = %self.config.max_items_per_scan, "Scanning need tx exec receipt upload records");

        // 查询DB中需要上传交易执行回执的记录
        let records = match ApiFeeRepo::scan_need_tx_exec_receipt_upload(
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
            let intent = FeeIntent::SideEffect(FeeSideEffectIntent::UploadTxExecReceipt(record.trade_no));
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
    /// 2. 基于事实状态，按照 AdvancementPoint 顺序检查可推进点
    /// 3. 找到第一个满足条件的推进点，生成对应意图
    /// 4. 发送意图并返回
    /// 
    /// TODO: extract to ShadowAdvancer
    /// Scanner should remain pure fact -> intent generator.
    /// try_advance performs one-shot advancement with side effects.
    ///
    /// 技术债：
    /// - try_advance 当前放在 ShadowScanner impl 中，语义上不够清晰
    /// - 未来理想形态：
    ///   - ShadowScanner: scan facts -> intents (只读)
    ///   - ShadowAdvancer: one-shot advance based on facts (可写)
    /// - 建议在 predicate 完全统一后进行重构
    pub async fn try_advance(&self, trade_no: &str) {
        info!(trade_no = %trade_no, "Try advancing fee transaction");

        // 查询最新的DB状态
        let fee = match ApiFeeRepo::get_api_fee_by_trade_no(
            &self.pool,
            trade_no,
        ).await {
            Ok(fee) => fee,
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, "Failed to get api fee by trade_no");
                return;
            }
        };

        // 按照 ADVANCEMENT_ORDER 顺序检查可推进点
        // 顺序与 scan_round 完全一致，确保行为一致性
        for point in ADVANCEMENT_ORDER {
            match point {
                AdvancementPoint::NeedTxAck if need_tx_ack(&fee) => {
                    info!(trade_no = %trade_no, "Need to send tx ACK");
                    let intent = FeeIntent::SideEffect(FeeSideEffectIntent::SendTxAck(trade_no.to_string()));
                    self.dispatch_intent(intent).await;
                    return;
                }
                AdvancementPoint::CanBuild if can_build(&fee) => {
                    info!(trade_no = %trade_no, "Can build transaction");
                    let intent = FeeIntent::Chain(FeeChainIntent::BuildTx(trade_no.to_string()));
                    self.dispatch_intent(intent).await;
                    return;
                }
                AdvancementPoint::CanBroadcast if can_broadcast(&fee) => {
                    info!(trade_no = %trade_no, "Can broadcast transaction");
                    let intent = FeeIntent::Chain(FeeChainIntent::BroadcastTx(trade_no.to_string()));
                    self.dispatch_intent(intent).await;
                    return;
                }
                AdvancementPoint::NeedTxExecReceiptUpload if need_tx_exec_receipt_upload(&fee) => {
                    info!(trade_no = %trade_no, "Need to upload tx exec receipt");
                    let intent = FeeIntent::SideEffect(FeeSideEffectIntent::UploadTxExecReceipt(trade_no.to_string()));
                    self.dispatch_intent(intent).await;
                    return;
                }
                AdvancementPoint::NeedTxResAck if need_tx_res_ack(&fee) => {
                    info!(trade_no = %trade_no, "Need to send tx res ACK");
                    let intent = FeeIntent::SideEffect(FeeSideEffectIntent::SendTxResAck(trade_no.to_string()));
                    self.dispatch_intent(intent).await;
                    return;
                }
                _ => continue,
            }
        }

        // 无可用推进点
        info!(trade_no = %trade_no, "No advancement possible based on current facts");
    }


}
