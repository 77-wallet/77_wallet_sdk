use smallvec::SmallVec;
use wallet_database::entities::api_withdraw::ApiWithdrawEntity;

use super::stage::AdvancementPoint;

#[derive(Debug, Clone, PartialEq)]
pub struct StageReason {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct StageEval {
    pub can_advance: bool,
    pub reasons: SmallVec<[StageReason; 4]>,
}

pub fn evaluate_point(point: AdvancementPoint, withdraw: &ApiWithdrawEntity) -> StageEval {
    match point {
        AdvancementPoint::NeedTxAck => evaluate_need_tx_ack(withdraw),
        AdvancementPoint::CanBuild => evaluate_can_build(withdraw),
        AdvancementPoint::CanBroadcast => evaluate_can_broadcast(withdraw),
        AdvancementPoint::NeedRecover => evaluate_need_recover(withdraw),
        AdvancementPoint::NeedTxExecReceiptUpload => evaluate_need_tx_exec_receipt_upload(withdraw),
        AdvancementPoint::NeedTxResAck => evaluate_need_tx_res_ack(withdraw),
        AdvancementPoint::FullyBlocked => {
            StageEval { can_advance: false, reasons: SmallVec::new() }
        }
    }
}

fn evaluate_need_tx_ack(withdraw: &ApiWithdrawEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if withdraw.tx_ack_sent_at.is_some() {
        reasons
            .push(StageReason { code: "tx_ack_sent", message: "Tx ACK already sent".to_string() });
    }
    if withdraw.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }
    if withdraw.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = withdraw.tx_ack_sent_at.is_none()
        && withdraw.finished_at.is_none()
        && withdraw.err_code.is_none();

    StageEval { can_advance, reasons }
}

fn evaluate_can_build(withdraw: &ApiWithdrawEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if withdraw.tx_ack_sent_at.is_none() {
        reasons.push(StageReason {
            code: "tx_ack_not_sent",
            message: "Tx ACK not sent yet".to_string(),
        });
    }
    if withdraw.audit_passed_at.is_none() {
        reasons.push(StageReason {
            code: "audit_not_passed",
            message: "Audit not passed yet".to_string(),
        });
    }
    if withdraw.raw_tx.is_some() {
        reasons.push(StageReason {
            code: "raw_tx_exists",
            message: "Raw tx already exists".to_string(),
        });
    }
    if withdraw.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }
    if withdraw.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = withdraw.tx_ack_sent_at.is_some()
        && withdraw.audit_passed_at.is_some()
        && withdraw.raw_tx.is_none()
        && withdraw.finished_at.is_none()
        && withdraw.err_code.is_none();

    StageEval { can_advance, reasons }
}

fn evaluate_can_broadcast(withdraw: &ApiWithdrawEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if withdraw.tx_ack_sent_at.is_none() {
        reasons.push(StageReason {
            code: "tx_ack_not_sent",
            message: "Tx ACK not sent yet".to_string(),
        });
    }
    if withdraw.raw_tx.is_none() {
        reasons.push(StageReason {
            code: "raw_tx_not_exists",
            message: "Raw tx not exists".to_string(),
        });
    }
    if withdraw.last_broadcast_at.is_some() {
        reasons.push(StageReason {
            code: "already_broadcasted",
            message: "Already broadcasted".to_string(),
        });
    }
    if withdraw.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }
    if withdraw.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = withdraw.tx_ack_sent_at.is_some()
        && withdraw.raw_tx.is_some()
        && withdraw.last_broadcast_at.is_none()
        && withdraw.finished_at.is_none()
        && withdraw.err_code.is_none();

    StageEval { can_advance, reasons }
}

fn evaluate_need_recover(withdraw: &ApiWithdrawEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if withdraw.tx_hash.is_none() {
        reasons.push(StageReason {
            code: "tx_hash_not_exists",
            message: "Tx hash not exists".to_string(),
        });
    }
    if withdraw.transaction_time.is_some() {
        reasons.push(StageReason {
            code: "transaction_time_exists",
            message: "Transaction time already exists".to_string(),
        });
    }
    if withdraw.last_broadcast_at.is_some() {
        reasons.push(StageReason {
            code: "already_broadcasted",
            message: "Already broadcasted".to_string(),
        });
    }
    if withdraw.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }
    if withdraw.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = withdraw.tx_hash.is_some()
        && withdraw.transaction_time.is_none()
        && withdraw.last_broadcast_at.is_none()
        && withdraw.finished_at.is_none()
        && withdraw.err_code.is_none();

    StageEval { can_advance, reasons }
}

fn evaluate_need_tx_exec_receipt_upload(withdraw: &ApiWithdrawEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if withdraw.tx_exec_receipt_uploaded_at.is_some() {
        reasons.push(StageReason {
            code: "receipt_uploaded",
            message: "Receipt already uploaded".to_string(),
        });
    }
    if withdraw.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }
    if withdraw.last_broadcast_at.is_none() && withdraw.err_code.is_none() {
        reasons.push(StageReason {
            code: "not_broadcasted",
            message: "Not broadcasted yet".to_string(),
        });
    }

    let can_advance = withdraw.finished_at.is_none()
        && withdraw.tx_exec_receipt_uploaded_at.is_none()
        && (withdraw.last_broadcast_at.is_some() || withdraw.err_code.is_some());

    StageEval { can_advance, reasons }
}

fn evaluate_need_tx_res_ack(withdraw: &ApiWithdrawEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if withdraw.tx_exec_receipt_uploaded_at.is_none() {
        reasons.push(StageReason {
            code: "receipt_not_uploaded",
            message: "Tx exec receipt not uploaded yet".to_string(),
        });
    }
    if withdraw.transaction_time.is_none() {
        reasons.push(StageReason {
            code: "tx_time_missing",
            message: "Transaction time missing".to_string(),
        });
    }
    if withdraw.tx_res_ack_sent_at.is_some() {
        reasons.push(StageReason {
            code: "tx_res_ack_sent",
            message: "Tx res ACK already sent".to_string(),
        });
    }
    if withdraw.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }
    if withdraw.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = withdraw.tx_exec_receipt_uploaded_at.is_some()
        && withdraw.transaction_time.is_some()
        && withdraw.tx_res_ack_sent_at.is_none()
        && withdraw.finished_at.is_none()
        && withdraw.err_code.is_none();

    StageEval { can_advance, reasons }
}
