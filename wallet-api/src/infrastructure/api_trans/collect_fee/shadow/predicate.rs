use smallvec::SmallVec;
use wallet_database::entities::api_fee::ApiFeeEntity;

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

pub fn evaluate_point(point: AdvancementPoint, fee: &ApiFeeEntity) -> StageEval {
    match point {
        AdvancementPoint::NeedTxAck => evaluate_need_tx_ack(fee),
        AdvancementPoint::CanBuild => evaluate_can_build(fee),
        AdvancementPoint::CanBroadcast => evaluate_can_broadcast(fee),
        AdvancementPoint::NeedRecover => evaluate_need_recover(fee),
        AdvancementPoint::NeedTxExecReceiptUpload => evaluate_need_tx_exec_receipt_upload(fee),
        AdvancementPoint::NeedTxResAck => evaluate_need_tx_res_ack(fee),
        AdvancementPoint::FullyBlocked => {
            StageEval { can_advance: false, reasons: SmallVec::new() }
        }
    }
}

fn evaluate_need_tx_ack(fee: &ApiFeeEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if fee.tx_ack_sent_at.is_some() {
        reasons
            .push(StageReason { code: "tx_ack_sent", message: "Tx ACK already sent".to_string() });
    }
    if fee.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }
    if fee.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance =
        fee.tx_ack_sent_at.is_none() && fee.finished_at.is_none() && fee.err_code.is_none();

    StageEval { can_advance, reasons }
}

fn evaluate_can_build(fee: &ApiFeeEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if fee.tx_ack_sent_at.is_none() {
        reasons.push(StageReason {
            code: "tx_ack_not_sent",
            message: "Tx ACK not sent yet".to_string(),
        });
    }
    if fee.raw_tx.is_some() {
        reasons.push(StageReason {
            code: "raw_tx_exists",
            message: "Raw tx already exists".to_string(),
        });
    }
    if fee.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }
    if fee.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = fee.tx_ack_sent_at.is_some()
        && fee.raw_tx.is_none()
        && fee.finished_at.is_none()
        && fee.err_code.is_none();

    StageEval { can_advance, reasons }
}

fn evaluate_can_broadcast(fee: &ApiFeeEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if fee.tx_ack_sent_at.is_none() {
        reasons.push(StageReason {
            code: "tx_ack_not_sent",
            message: "Tx ACK not sent yet".to_string(),
        });
    }
    if fee.raw_tx.is_none() {
        reasons.push(StageReason {
            code: "raw_tx_not_exists",
            message: "Raw tx not exists".to_string(),
        });
    }
    if fee.last_broadcast_at.is_some() {
        reasons.push(StageReason {
            code: "already_broadcasted",
            message: "Already broadcasted".to_string(),
        });
    }
    if fee.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }
    if fee.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = fee.tx_ack_sent_at.is_some()
        && fee.raw_tx.is_some()
        && fee.last_broadcast_at.is_none()
        && fee.finished_at.is_none()
        && fee.err_code.is_none();

    StageEval { can_advance, reasons }
}

fn evaluate_need_recover(fee: &ApiFeeEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if fee.tx_hash.is_none() {
        reasons.push(StageReason {
            code: "tx_hash_not_exists",
            message: "Tx hash not exists".to_string(),
        });
    }
    if fee.transaction_time.is_some() {
        reasons.push(StageReason {
            code: "transaction_time_exists",
            message: "Transaction time already exists".to_string(),
        });
    }
    if fee.last_broadcast_at.is_some() {
        reasons.push(StageReason {
            code: "already_broadcasted",
            message: "Already broadcasted".to_string(),
        });
    }
    if fee.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }
    if fee.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = fee.tx_hash.is_some()
        && fee.transaction_time.is_none()
        && fee.last_broadcast_at.is_none()
        && fee.finished_at.is_none()
        && fee.err_code.is_none();

    StageEval { can_advance, reasons }
}

fn evaluate_need_tx_exec_receipt_upload(fee: &ApiFeeEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if fee.tx_exec_receipt_uploaded_at.is_some() {
        reasons.push(StageReason {
            code: "receipt_uploaded",
            message: "Receipt already uploaded".to_string(),
        });
    }
    if fee.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }
    if fee.last_broadcast_at.is_none() && fee.err_code.is_none() {
        reasons.push(StageReason {
            code: "not_broadcasted",
            message: "Not broadcasted yet".to_string(),
        });
    }

    let can_advance = fee.finished_at.is_none()
        && fee.tx_exec_receipt_uploaded_at.is_none()
        && (fee.last_broadcast_at.is_some() || fee.err_code.is_some());

    StageEval { can_advance, reasons }
}

fn evaluate_need_tx_res_ack(fee: &ApiFeeEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if fee.transaction_time.is_none() {
        reasons.push(StageReason {
            code: "transaction_time_not_exists",
            message: "Transaction time not exists".to_string(),
        });
    }

    if fee.tx_res_ack_sent_at.is_some() {
        reasons.push(StageReason {
            code: "tx_res_ack_sent",
            message: "Tx res ACK already sent".to_string(),
        });
    }
    if fee.finished_at.is_some() {
        reasons
            .push(StageReason { code: "finished", message: "Order already finished".to_string() });
    }
    if fee.err_code.is_some() {
        reasons.push(StageReason { code: "error", message: "Order has error".to_string() });
    }

    let can_advance = fee.transaction_time.is_some()
        && fee.tx_res_ack_sent_at.is_none()
        && fee.finished_at.is_none()
        && fee.err_code.is_none();

    StageEval { can_advance, reasons }
}
