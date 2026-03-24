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

fn is_evm_chain_code(chain_code: &str) -> bool {
    chain_code.eq_ignore_ascii_case("eth") || chain_code.eq_ignore_ascii_case("bnb")
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
    if is_evm_chain_code(&withdraw.chain_code) && withdraw.broadcast_uncertain_since_at.is_some() {
        reasons.push(StageReason {
            code: "evm_broadcast_uncertain_in_progress",
            message: "EVM tx is in uncertain state; recover owns progression".to_string(),
        });
    }

    let can_advance = withdraw.tx_ack_sent_at.is_some()
        && withdraw.raw_tx.is_some()
        && withdraw.last_broadcast_at.is_none()
        && (!is_evm_chain_code(&withdraw.chain_code)
            || withdraw.broadcast_uncertain_since_at.is_none())
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
    if withdraw.tx_exec_receipt_uploaded_at.is_some() {
        reasons.push(StageReason {
            code: "tx_exec_receipt_uploaded",
            message: "Tx exec receipt already uploaded; auto recover disabled".to_string(),
        });
    }
    if is_evm_chain_code(&withdraw.chain_code)
        && withdraw.raw_tx.is_some()
        && withdraw.last_broadcast_at.is_none()
        && withdraw.broadcast_uncertain_since_at.is_none()
    {
        reasons.push(StageReason {
            code: "evm_broadcast_not_attempted",
            message: "EVM raw_tx exists but no uncertain marker; broadcast should proceed first"
                .to_string(),
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
        && withdraw.tx_exec_receipt_uploaded_at.is_none()
        && !(is_evm_chain_code(&withdraw.chain_code)
            && withdraw.raw_tx.is_some()
            && withdraw.last_broadcast_at.is_none()
            && withdraw.broadcast_uncertain_since_at.is_none())
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
    let has_confirmed_or_failed_result = withdraw.chain_success_at.is_some()
        || withdraw.transaction_time.is_some()
        || withdraw.chain_failed_at.is_some()
        || withdraw.err_code.is_some();
    if !has_confirmed_or_failed_result {
        reasons.push(StageReason {
            code: "execution_result_unconfirmed",
            message: if withdraw.last_broadcast_at.is_some() {
                "Broadcast visible, waiting for chain confirmation".to_string()
            } else {
                "Execution result not confirmed yet".to_string()
            },
        });
    }

    let can_advance = withdraw.finished_at.is_none()
        && withdraw.tx_exec_receipt_uploaded_at.is_none()
        && has_confirmed_or_failed_result;

    StageEval { can_advance, reasons }
}

fn evaluate_need_tx_res_ack(withdraw: &ApiWithdrawEntity) -> StageEval {
    let mut reasons = SmallVec::new();

    if withdraw.tx_res_received_at.is_none() {
        reasons.push(StageReason {
            code: "tx_res_not_received",
            message: "SER tx result push (AWM_ORDER_TRANS_RES) not received".to_string(),
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

    let can_advance = withdraw.tx_res_received_at.is_some()
        && withdraw.transaction_time.is_some()
        && withdraw.tx_res_ack_sent_at.is_none()
        && withdraw.finished_at.is_none()
        && withdraw.err_code.is_none();

    StageEval { can_advance, reasons }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use wallet_database::entities::{
        api_trade_type::ApiTradeType,
        api_withdraw::{ApiWithdrawStatus, WithdrawFailureStage},
        asset_token_key::AssetTokenKey,
    };

    fn base_withdraw() -> ApiWithdrawEntity {
        ApiWithdrawEntity {
            id: 1,
            name: "n".to_string(),
            uid: "u".to_string(),
            from_addr: "from".to_string(),
            to_addr: "to".to_string(),
            value: "0".to_string(),
            validate: "v".to_string(),
            chain_code: "tron".to_string(),
            token_addr: AssetTokenKey::Native,
            symbol: "s".to_string(),
            trade_no: "W_TEST".to_string(),
            trade_type: ApiTradeType::Withdraw,
            init_status: ApiWithdrawStatus::Init,
            status: ApiWithdrawStatus::Init,
            nonce: 0,
            tx_hash: Some("h".to_string()),
            raw_tx: Some("{}".to_string()),
            resource_consume: "0".to_string(),
            transaction_fee: "0".to_string(),
            transaction_time: None,
            block_height: None,
            notes: None,
            post_tx_count: 0,
            post_confirm_tx_count: 0,
            err_code: None,
            err_msg: None,
            tx_ack_sent_at: Some(Utc::now()),
            building_at: None,
            last_broadcast_at: None,
            broadcast_uncertain_since_at: None,
            broadcast_uncertain_retry_count: 0,
            broadcast_uncertain_last_checked_at: None,
            broadcast_uncertain_reconciled_at: None,
            broadcast_uncertain_rebroadcast_count: 0,
            tx_res_ack_sent_at: None,
            tx_res_received_at: None,
            tx_exec_receipt_uploaded_at: Some(Utc::now()),
            finished_at: None,
            audit_passed_at: Some(Utc::now()),
            audit_rejected_at: None,
            audit_reason: None,
            chain_success_at: None,
            chain_failed_at: None,
            failure_stage: Some(WithdrawFailureStage::Unknown),
            created_at: Utc::now(),
            updated_at: Some(Utc::now()),
        }
    }

    #[test]
    fn need_tx_res_ack_requires_tx_res_received_at() {
        let mut w = base_withdraw();
        w.transaction_time = Some(Utc::now());
        w.tx_res_received_at = None;

        let eval = evaluate_point(AdvancementPoint::NeedTxResAck, &w);
        assert!(!eval.can_advance);

        w.tx_res_received_at = Some(Utc::now());
        let eval2 = evaluate_point(AdvancementPoint::NeedTxResAck, &w);
        assert!(eval2.can_advance);
    }

    #[test]
    fn need_tx_exec_receipt_upload_allows_transaction_time_without_last_broadcast() {
        let mut w = base_withdraw();
        w.tx_exec_receipt_uploaded_at = None;
        w.last_broadcast_at = None;
        w.transaction_time = Some(Utc::now());

        let eval = evaluate_point(AdvancementPoint::NeedTxExecReceiptUpload, &w);
        assert!(eval.can_advance);
    }

    #[test]
    fn need_tx_exec_receipt_upload_rejects_broadcast_only_pending() {
        let mut w = base_withdraw();
        w.tx_exec_receipt_uploaded_at = None;
        w.last_broadcast_at = Some(Utc::now());

        let eval = evaluate_point(AdvancementPoint::NeedTxExecReceiptUpload, &w);
        assert!(!eval.can_advance);
        assert!(eval.reasons.iter().any(|r| r.code == "execution_result_unconfirmed"));
    }

    #[test]
    fn need_recover_allows_broadcasted_tx_without_transaction_time() {
        let mut w = base_withdraw();
        w.tx_exec_receipt_uploaded_at = None;
        w.last_broadcast_at = Some(Utc::now());

        let eval = evaluate_point(AdvancementPoint::NeedRecover, &w);
        assert!(eval.can_advance);
    }
}
