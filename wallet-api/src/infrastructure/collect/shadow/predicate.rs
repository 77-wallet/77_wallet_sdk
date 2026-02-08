use smallvec::SmallVec;
use wallet_database::entities::api_collect::ApiCollectEntity;

use super::stage::CollectStage;

/// 推进原因
#[derive(Debug, Clone, PartialEq)]
pub struct StageReason {
    pub code: &'static str,
    pub message: String,
}

/// 阶段评估结果
#[derive(Debug, Clone)]
pub struct StageEval {
    pub can_advance: bool,
    pub reasons: SmallVec<[StageReason; 4]>,
}

/// 评估指定阶段
pub fn evaluate_stage(stage: CollectStage, collect: &ApiCollectEntity) -> StageEval {
    match stage {
        CollectStage::NeedOrderAck => evaluate_need_order_ack(collect),
        CollectStage::CanBuild => evaluate_can_build(collect),
        CollectStage::NeedTxFeeResAck => evaluate_need_tx_fee_res_ack(collect),
        CollectStage::CanBroadcast => evaluate_can_broadcast(collect),
        CollectStage::NeedRecover => evaluate_need_recover(collect),
        CollectStage::NeedTxExecReceiptUpload => evaluate_need_tx_exec_receipt_upload(collect),
        CollectStage::NeedResultAck => evaluate_need_result_ack(collect),
        CollectStage::NeedServiceFeeUpload => evaluate_need_service_fee_upload(collect),
        CollectStage::FullyBlocked => StageEval { can_advance: false, reasons: SmallVec::new() },
    }
}

/// 评估 NeedOrderAck 阶段
/// 检查是否需要发送订单 ACK
///
/// 事实条件：
/// - order_ack_sent_at IS NULL
/// - finished_at IS NULL
/// - err_code IS NULL
///
/// ⚠️ 重要说明：
/// - Scanner 不得在 finished_at IS NOT NULL 的记录上产生任何动作
/// - 确保在已终态的记录上不会再尝试发送订单 ACK
/// - 一旦 err_code IS NOT NULL，不再产生任何推进意图
fn evaluate_need_order_ack(collect: &ApiCollectEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if collect.order_ack_sent_at.is_some() {
        reasons.push(StageReason {
            code: "order_ack_sent",
            message: "Order ACK already sent".to_string(),
        });
    }

    if collect.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }

    if collect.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = collect.order_ack_sent_at.is_none()
        && collect.finished_at.is_none()
        && collect.err_code.is_none();

    StageEval { can_advance, reasons }
}

/// 评估 CanBuild 阶段
/// 检查是否可以构建交易
///
/// 事实条件（强顺序屏障）：
/// - order_ack_sent_at IS NOT NULL   // 订单确认已完成
/// - raw_tx IS NULL
/// - need_service_fee != true
fn evaluate_can_build(collect: &ApiCollectEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if collect.order_ack_sent_at.is_none() {
        reasons.push(StageReason {
            code: "order_ack_not_sent",
            message: "Order ACK not sent yet".to_string(),
        });
    }

    if collect.raw_tx.is_some() {
        reasons.push(StageReason {
            code: "raw_tx_exists",
            message: "Raw tx already exists".to_string(),
        });
    }

    if collect.need_service_fee == Some(true) {
        reasons.push(StageReason {
            code: "need_service_fee",
            message: "Need service fee".to_string(),
        });
    }

    if collect.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = collect.order_ack_sent_at.is_some()
        && collect.raw_tx.is_none()
        && collect.need_service_fee != Some(true)
        && collect.err_code.is_none();

    StageEval { can_advance, reasons }
}

/// 评估 NeedTxFeeResAck 阶段
fn evaluate_need_tx_fee_res_ack(collect: &ApiCollectEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if collect.need_service_fee == Some(true) {
        reasons.push(StageReason {
            code: "need_service_fee",
            message: "Still need service fee".to_string(),
        });
    }

    if collect.ever_needed_service_fee != true {
        reasons.push(StageReason {
            code: "never_needed_service_fee",
            message: "Never needed service fee".to_string(),
        });
    }

    if collect.tx_fee_res_ack_sent_at.is_some() {
        reasons.push(StageReason {
            code: "tx_fee_res_ack_sent",
            message: "Tx fee res ACK already sent".to_string(),
        });
    }

    if collect.last_broadcast_at.is_some() {
        reasons.push(StageReason {
            code: "already_broadcasted",
            message: "Already broadcasted".to_string(),
        });
    }

    if collect.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }

    if collect.transaction_time.is_some() {
        reasons.push(StageReason {
            code: "transaction_time_exists",
            message: "Transaction time already exists".to_string(),
        });
    }

    if collect.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = collect.need_service_fee != Some(true)
        && collect.ever_needed_service_fee == true
        && collect.tx_fee_res_ack_sent_at.is_none()
        && collect.last_broadcast_at.is_none()
        && collect.finished_at.is_none()
        && collect.transaction_time.is_none()
        && collect.err_code.is_none();

    StageEval { can_advance, reasons }
}

/// 评估 CanBroadcast 阶段
/// 检查是否可以广播交易
///
/// 事实条件：
/// - raw_tx IS NOT NULL
/// - last_broadcast_at IS NULL
/// - finished_at IS NULL
/// - AND (
///     - ever_needed_service_fee = false
///     - OR tx_fee_res_ack_sent_at IS NOT NULL
///   )
///
/// ⚠️ 语义：
/// - 从未因手续费失败过的交易：可直接广播
/// - 曾经因手续费失败过的交易：必须先完成 TxFeeResAck，才能广播
fn evaluate_can_broadcast(collect: &ApiCollectEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if collect.raw_tx.is_none() {
        reasons.push(StageReason {
            code: "raw_tx_not_exists",
            message: "Raw tx not exists".to_string(),
        });
    }

    if collect.last_broadcast_at.is_some() {
        reasons.push(StageReason {
            code: "already_broadcasted",
            message: "Already broadcasted".to_string(),
        });
    }

    if collect.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }

    if collect.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    if collect.ever_needed_service_fee == true && collect.tx_fee_res_ack_sent_at.is_none() {
        reasons.push(StageReason {
            code: "tx_fee_res_ack_not_sent",
            message: "Tx fee res ACK not sent yet".to_string(),
        });
    }

    let can_advance = collect.raw_tx.is_some()
        && collect.last_broadcast_at.is_none()
        && collect.finished_at.is_none()
        && collect.err_code.is_none()
        && (collect.ever_needed_service_fee == false || collect.tx_fee_res_ack_sent_at.is_some());

    StageEval { can_advance, reasons }
}

/// 评估 NeedRecover 阶段
/// 检查是否需要恢复交易
///
/// 事实条件：
/// - tx_hash IS NOT NULL
/// - transaction_time IS NULL
/// - last_broadcast_at IS NULL
/// - finished_at IS NULL
/// - err_code IS NULL
///
/// ⚠️ 重要说明：
/// - Recover 的目的是补全链上结果事实
/// - 添加 last_broadcast_at IS NULL 条件，避免与回执上传竞争
/// - 只看不可逆事实是否缺失，不做时间推断
fn evaluate_need_recover(collect: &ApiCollectEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if collect.tx_hash.is_none() {
        reasons.push(StageReason {
            code: "tx_hash_not_exists",
            message: "Tx hash not exists".to_string(),
        });
    }

    if collect.transaction_time.is_some() {
        reasons.push(StageReason {
            code: "transaction_time_exists",
            message: "Transaction time already exists".to_string(),
        });
    }

    if collect.last_broadcast_at.is_some() {
        reasons.push(StageReason {
            code: "already_broadcasted",
            message: "Already broadcasted".to_string(),
        });
    }

    if collect.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }

    if collect.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = collect.tx_hash.is_some()
        && collect.transaction_time.is_none()
        && collect.last_broadcast_at.is_none()
        && collect.finished_at.is_none()
        && collect.err_code.is_none();

    StageEval { can_advance, reasons }
}

/// 评估 NeedTxExecReceiptUpload 阶段
/// 检查是否需要上传交易执行回执
///
/// 事实条件：
/// - last_broadcast_at IS NOT NULL
/// - tx_exec_receipt_uploaded_at IS NULL
/// - finished_at IS NULL
///
/// ⚠️ Recover 保证：
/// - 若 transaction_time 被 Recover 写入
/// - last_broadcast_at 必须已被补写或随后补写
/// - Scanner 本身不负责兜底
///
/// ⚠️ 重要约束：
/// - UploadTxExecReceipt 必须在成功 / 失败路径都触发
/// - 生命周期收口（finished_at）只能由 Worker 在副作用完成后写入
/// - 此为 err_code 失败冻结态的唯一例外
/// - 但仍然受 finished_at 终态屏障约束
fn evaluate_need_tx_exec_receipt_upload(collect: &ApiCollectEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if collect.last_broadcast_at.is_none() {
        reasons.push(StageReason {
            code: "not_broadcasted",
            message: "Not broadcasted yet".to_string(),
        });
    }

    if collect.tx_exec_receipt_uploaded_at.is_some() {
        reasons.push(StageReason {
            code: "receipt_uploaded",
            message: "Receipt already uploaded".to_string(),
        });
    }

    if collect.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }

    let can_advance = collect.last_broadcast_at.is_some()
        && collect.tx_exec_receipt_uploaded_at.is_none()
        && collect.finished_at.is_none();

    StageEval { can_advance, reasons }
}

/// 评估 NeedResultAck 阶段
/// 检查是否需要发送结果 ACK
///
/// 事实条件：
/// - transaction_time IS NOT NULL
/// - result_ack_sent_at IS NULL
/// - finished_at IS NULL
/// - err_code IS NULL
///
/// ⚠️ 重要说明：
/// - ResultAck 仅用于“成功结果确认”
/// - 失败结果通过 err_code 事实本身表达，不再发送 ResultAck
/// - 一旦 err_code IS NOT NULL，不再产生 ResultAck 意图
fn evaluate_need_result_ack(collect: &ApiCollectEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if collect.transaction_time.is_none() {
        reasons.push(StageReason {
            code: "transaction_time_not_exists",
            message: "Transaction time not exists".to_string(),
        });
    }

    if collect.result_ack_sent_at.is_some() {
        reasons.push(StageReason {
            code: "result_ack_sent",
            message: "Result ACK already sent".to_string(),
        });
    }

    if collect.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }

    if collect.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = collect.transaction_time.is_some()
        && collect.result_ack_sent_at.is_none()
        && collect.finished_at.is_none()
        && collect.err_code.is_none();

    StageEval { can_advance, reasons }
}

/// 评估 NeedServiceFeeUpload 阶段
/// 检查是否需要上传服务费
///
/// 事实条件：
/// - need_service_fee = true
/// - service_fee_uploaded_at IS NULL
/// - err_code IS NULL
///
/// ⚠️ 重要说明：
/// - UploadServiceFee 只在构建阶段的可恢复失败路径触发
/// - 一旦发生不可逆执行失败（err_code IS NOT NULL），不再允许上传服务费
fn evaluate_need_service_fee_upload(collect: &ApiCollectEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if collect.need_service_fee != Some(true) {
        reasons.push(StageReason {
            code: "no_need_service_fee",
            message: "No need service fee".to_string(),
        });
    }

    if collect.service_fee_uploaded_at.is_some() {
        reasons.push(StageReason {
            code: "service_fee_uploaded",
            message: "Service fee already uploaded".to_string(),
        });
    }

    if collect.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = collect.need_service_fee == Some(true)
        && collect.service_fee_uploaded_at.is_none()
        && collect.err_code.is_none();

    StageEval { can_advance, reasons }
}

/// 快速检查是否有任何推进点
pub fn has_any_advancement_point(collect: &ApiCollectEntity) -> bool {
    use super::stage::COLLECT_ADVANCEMENT_ORDER;

    COLLECT_ADVANCEMENT_ORDER.iter().any(|stage| evaluate_stage(*stage, collect).can_advance)
}

/// 快速检查是否可能卡住
pub fn is_potentially_blocked(collect: &ApiCollectEntity) -> bool {
    // 已完成或有错误的订单不视为卡住
    if collect.finished_at.is_some() || collect.err_code.is_some() {
        return false;
    }

    // 没有推进点的订单视为可能卡住
    !has_any_advancement_point(collect)
}
