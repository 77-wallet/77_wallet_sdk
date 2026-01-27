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
use std::time::{Duration, Instant};

use tracing::{error, info, warn};
use wallet_database::{CollectDbPool, entities::api_collect::ApiCollectEntity};

use crate::infrastructure::collect::shadow::{ChainIntent, SideEffectIntent};

use super::CollectIntent;

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
    /// 需要发送订单确认 ACK
    NeedOrderAck,
    /// 可以构建交易
    CanBuild,
    /// 可以广播交易
    CanBroadcast,
    /// 需要上传交易执行回执
    NeedTxExecReceiptUpload,
    /// 需要发送结果确认 ACK
    NeedResultAck,
    /// 需要上传服务费
    NeedServiceFeeUpload,
}

/// 推进点顺序常量
/// - 顺序与 scan_round 完全一致
/// - try_advance 必须使用此常量，确保行为一致性
pub const ADVANCEMENT_ORDER: &[AdvancementPoint] = &[
    AdvancementPoint::NeedOrderAck,
    AdvancementPoint::CanBuild,
    AdvancementPoint::CanBroadcast,
    AdvancementPoint::NeedTxExecReceiptUpload,
    AdvancementPoint::NeedResultAck,
    AdvancementPoint::NeedServiceFeeUpload,
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
/// - order_ack_sent_at IS NOT NULL   // 订单确认已完成
/// - raw_tx IS NULL
/// - build_blocked_at IS NULL
fn can_build(collect: &ApiCollectEntity) -> bool {
    collect.order_ack_sent_at.is_some() && collect.raw_tx.is_none() && collect.build_blocked_at.is_none()
}

/// 检查是否可以广播交易
///
/// 事实条件：
/// - raw_tx IS NOT NULL
/// - transaction_time IS NULL
fn can_broadcast(collect: &ApiCollectEntity) -> bool {
    collect.raw_tx.is_some() && collect.transaction_time.is_none()
}

/// 副作用类（Side Effect）predicate
/// ----------------------------------------------------------------------------

/// 检查是否需要发送订单 ACK
///
/// 事实条件：
/// - order_ack_sent_at IS NULL
fn need_order_ack(collect: &ApiCollectEntity) -> bool {
    collect.order_ack_sent_at.is_none()
}

/// 检查是否需要上传交易执行回执
///
/// 事实条件：
/// - transaction_time IS NOT NULL
/// - tx_exec_receipt_uploaded_at IS NULL
fn need_tx_exec_receipt_upload(collect: &ApiCollectEntity) -> bool {
    collect.transaction_time.is_some() && collect.tx_exec_receipt_uploaded_at.is_none()
}

/// 检查是否需要发送结果 ACK
///
/// 事实条件：
/// - tx_exec_receipt_uploaded_at IS NOT NULL
/// - result_ack_sent_at IS NULL
fn need_result_ack(collect: &ApiCollectEntity) -> bool {
    collect.tx_exec_receipt_uploaded_at.is_some() && collect.result_ack_sent_at.is_none()
}

/// 检查是否需要上传服务费
///
/// 事实条件：
/// - transaction_time IS NOT NULL
/// - service_fee_uploaded_at IS NULL
fn need_service_fee_upload(collect: &ApiCollectEntity) -> bool {
    collect.transaction_time.is_some() && collect.service_fee_uploaded_at.is_none()
}

/// 终态 / 完成判断（Future Use）
/// ----------------------------------------------------------------------------

/// 检查交易是否已完成所有链事实
///
/// 事实条件：
/// - transaction_time IS NOT NULL
fn is_chain_finished(collect: &ApiCollectEntity) -> bool {
    collect.transaction_time.is_some()
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
    /// - build_blocked_at IS NULL        // 系统级背压未激活
    ///
    /// ⚠️ 设计说明：
    /// BuildTx 必须显式依赖 OrderAck 完成，
    /// 禁止移除 order_ack_sent_at 条件，否则会破坏强顺序保证。
    ///
    /// 注意：
    /// - build_blocked_at 是系统级背压机制
    /// - 一旦设置，scanner 不应再生成 build intent
    /// - 直到被外部条件显式清除
    /// - 用于跨 scan round 的系统级背压
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
    ///
    /// SQL must be equivalent to can_broadcast()
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
    ///
    /// SQL must be equivalent to need_result_ack()
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
    ///
    /// SQL must be equivalent to need_service_fee_upload()
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
    ///
    /// SQL must be equivalent to need_tx_exec_receipt_upload()
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
    ///
    /// SQL must be equivalent to need_order_ack()
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

    /// 尝试基于当前事实推进一个阶段
    /// 
    /// 注意：try_advance 每次最多推进一个阶段
    /// 多阶段推进依赖后续 Tick 或定时扫描
    /// 
    /// 参数：
    /// - trade_no: 归集交易编号
    /// 
    /// 行为：
    /// 1. 查询最新的DB状态
    /// 2. 基于事实状态，按照 ADVANCEMENT_ORDER 顺序检查可推进点
    /// 3. 找到第一个满足条件的推进点，生成对应意图
    /// 4. 发送意图并返回
    /// 
    /// 技术债：
    /// - try_advance 当前放在 ShadowScanner impl 中，语义上不够清晰
    /// - 未来理想形态：
    ///   - ShadowScanner: scan facts -> intents (只读)
    ///   - ShadowAdvancer: one-shot advance based on facts (可写)
    /// - 建议在 predicate 完全统一后进行重构
    ///
    /// TODO: extract to ShadowAdvancer
    /// Scanner should remain pure fact -> intent generator.
    /// try_advance performs one-shot advancement with side effects.
    ///
    /// 重要约束：
    /// - try_advance 的推进顺序必须与 scan_round 完全一致
    /// - 不允许出现 "try_advance 能推进但 scan_round 不会 scan 到" 的阶段
    pub async fn try_advance(&self, trade_no: &str) {
        info!(trade_no = %trade_no, "Try advancing collect transaction");

        // 查询最新的DB状态
        let collect = match wallet_database::repositories::api_wallet::collect::ApiCollectRepo::get_api_collect_by_trade_no(
            &self.pool,
            trade_no,
        ).await {
            Ok(collect) => collect,
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, "Failed to get api collect by trade_no");
                return;
            }
        };

        // 按照 ADVANCEMENT_ORDER 顺序检查可推进点
        // 顺序与 scan_round 完全一致，确保行为一致性
        for point in ADVANCEMENT_ORDER {
            match point {
                AdvancementPoint::NeedOrderAck if need_order_ack(&collect) => {
                    info!(trade_no = %trade_no, "Need to send order ACK");
                    let intent = CollectIntent::SideEffect(SideEffectIntent::SendOrderAck(trade_no.to_string()));
                    self.dispatch_intent(intent).await;
                    return;
                }
                AdvancementPoint::CanBuild if can_build(&collect) => {
                    info!(trade_no = %trade_no, "Can build transaction");
                    let intent = CollectIntent::Chain(ChainIntent::BuildTx(trade_no.to_string()));
                    self.dispatch_intent(intent).await;
                    return;
                }
                AdvancementPoint::CanBroadcast if can_broadcast(&collect) => {
                    info!(trade_no = %trade_no, "Can broadcast transaction");
                    let intent = CollectIntent::Chain(ChainIntent::BroadcastTx(trade_no.to_string()));
                    self.dispatch_intent(intent).await;
                    return;
                }
                AdvancementPoint::NeedTxExecReceiptUpload if need_tx_exec_receipt_upload(&collect) => {
                    info!(trade_no = %trade_no, "Need to upload tx exec receipt");
                    let intent = CollectIntent::SideEffect(SideEffectIntent::UploadTxExecReceipt(trade_no.to_string()));
                    self.dispatch_intent(intent).await;
                    return;
                }
                AdvancementPoint::NeedResultAck if need_result_ack(&collect) => {
                    info!(trade_no = %trade_no, "Need to send result ACK");
                    let intent = CollectIntent::SideEffect(SideEffectIntent::SendResultAck(trade_no.to_string()));
                    self.dispatch_intent(intent).await;
                    return;
                }
                AdvancementPoint::NeedServiceFeeUpload if need_service_fee_upload(&collect) => {
                    info!(trade_no = %trade_no, "Need to upload service fee");
                    let intent = CollectIntent::SideEffect(SideEffectIntent::UploadServiceFee(trade_no.to_string()));
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
