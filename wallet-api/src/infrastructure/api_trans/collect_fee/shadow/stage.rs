#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdvancementPoint {
    NeedTxAck,
    CanBuild,
    CanBroadcast,
    NeedRecover,
    NeedTxExecReceiptUpload,
    NeedTxResAck,
    FullyBlocked,
}

/// 推进点顺序常量
/// - 顺序与 scan_round 完全一致
/// - try_advance 必须使用此常量，确保行为一致性
pub const ADVANCEMENT_ORDER: &[AdvancementPoint] = &[
    AdvancementPoint::NeedTxAck,
    AdvancementPoint::CanBuild,
    AdvancementPoint::CanBroadcast,
    AdvancementPoint::NeedRecover,
    AdvancementPoint::NeedTxExecReceiptUpload,
    AdvancementPoint::NeedTxResAck,
];

pub trait StageQueryBuilder {
    fn sql_filter(point: AdvancementPoint) -> String;
    fn rust_predicate(
        point: AdvancementPoint,
    ) -> fn(&wallet_database::entities::api_fee::ApiFeeEntity) -> bool;
}

pub struct DefaultStageQueryBuilder;

impl StageQueryBuilder for DefaultStageQueryBuilder {
    fn sql_filter(point: AdvancementPoint) -> String {
        match point {
            AdvancementPoint::NeedTxAck => "tx_ack_sent_at IS NULL".to_string(),
            AdvancementPoint::CanBuild => {
                "tx_ack_sent_at IS NOT NULL AND raw_tx IS NULL".to_string()
            }
            AdvancementPoint::CanBroadcast => {
                "raw_tx IS NOT NULL AND last_broadcast_at IS NULL AND transaction_time IS NULL AND finished_at IS NULL".to_string()
            }
            AdvancementPoint::NeedRecover => {
                "tx_hash IS NOT NULL AND transaction_time IS NULL AND tx_exec_receipt_uploaded_at IS NULL AND finished_at IS NULL AND err_code IS NULL".to_string()
            }
            AdvancementPoint::NeedTxExecReceiptUpload => {
                "tx_exec_receipt_uploaded_at IS NULL AND finished_at IS NULL AND (err_code IS NOT NULL OR transaction_time IS NOT NULL)".to_string()
            }
            AdvancementPoint::NeedTxResAck => {
                "transaction_time IS NOT NULL AND tx_res_ack_sent_at IS NULL AND finished_at IS NULL".to_string()
            }
            AdvancementPoint::FullyBlocked => "".to_string(),
        }
    }

    fn rust_predicate(
        point: AdvancementPoint,
    ) -> fn(&wallet_database::entities::api_fee::ApiFeeEntity) -> bool {
        match point {
            AdvancementPoint::NeedTxAck => |fee| fee.tx_ack_sent_at.is_none(),
            AdvancementPoint::CanBuild => {
                |fee| fee.tx_ack_sent_at.is_some() && fee.raw_tx.is_none()
            }
            AdvancementPoint::CanBroadcast => |fee| {
                fee.raw_tx.is_some()
                    && fee.last_broadcast_at.is_none()
                    && fee.transaction_time.is_none()
                    && fee.finished_at.is_none()
                    && fee.err_code.is_none()
            },
            AdvancementPoint::NeedRecover => |fee| {
                fee.tx_hash.is_some()
                    && fee.transaction_time.is_none()
                    && fee.tx_exec_receipt_uploaded_at.is_none()
                    && fee.finished_at.is_none()
                    && fee.err_code.is_none()
            },
            AdvancementPoint::NeedTxExecReceiptUpload => |fee| {
                fee.tx_exec_receipt_uploaded_at.is_none()
                    && fee.finished_at.is_none()
                    && (fee.err_code.is_some() || fee.transaction_time.is_some())
            },
            AdvancementPoint::NeedTxResAck => |fee| {
                fee.transaction_time.is_some()
                    && fee.tx_res_ack_sent_at.is_none()
                    && fee.finished_at.is_none()
            },
            AdvancementPoint::FullyBlocked => |_| false,
        }
    }
}

impl AdvancementPoint {
    pub fn base_severity(&self) -> u8 {
        match self {
            AdvancementPoint::NeedTxAck => 1,
            AdvancementPoint::CanBuild => 0,
            AdvancementPoint::CanBroadcast => 0,
            AdvancementPoint::NeedRecover => 2,
            AdvancementPoint::NeedTxExecReceiptUpload => 1,
            AdvancementPoint::NeedTxResAck => 1,
            AdvancementPoint::FullyBlocked => 4,
        }
    }

    pub fn wait_threshold_minutes(&self) -> i64 {
        match self {
            AdvancementPoint::NeedTxAck => 2,
            AdvancementPoint::CanBuild => 5,
            AdvancementPoint::CanBroadcast => 5,
            AdvancementPoint::NeedRecover => 10,
            AdvancementPoint::NeedTxExecReceiptUpload => 15,
            AdvancementPoint::NeedTxResAck => 8,
            AdvancementPoint::FullyBlocked => 1,
        }
    }

    pub fn next_expected_fact(&self) -> &'static str {
        match self {
            AdvancementPoint::NeedTxAck => "tx_ack_sent_at",
            AdvancementPoint::CanBuild => "raw_tx",
            AdvancementPoint::CanBroadcast => "last_broadcast_at",
            AdvancementPoint::NeedRecover => "transaction_time",
            AdvancementPoint::NeedTxExecReceiptUpload => "tx_exec_receipt_uploaded_at",
            AdvancementPoint::NeedTxResAck => "tx_res_ack_sent_at",
            AdvancementPoint::FullyBlocked => "finished_at",
        }
    }
}
