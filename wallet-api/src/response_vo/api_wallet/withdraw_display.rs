use serde_repr::Serialize_repr;
use wallet_database::entities::api_withdraw::{ApiWithdrawStatus, ErrCode, WithdrawFailureStage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr)]
#[repr(i32)]
pub enum FailureReasonDisplay {
    /// 未知失败
    UnknownFailed = 0,
    /// 审核拒绝
    AuditRejected = 1,
    /// 签名失败
    SignFailed = 2,
    /// 广播失败
    BroadcastFailed = 3,
    /// 链上失败
    ChainFailed = 4,
    /// 余额/手续费不足
    ResourceFailed = 5,
}

impl FailureReasonDisplay {
    /// 从状态、错误码和失败阶段推断失败原因类型
    pub fn from_status_and_error(
        status: ApiWithdrawStatus,
        err_code: Option<ErrCode>,
        failure_stage: Option<WithdrawFailureStage>,
    ) -> Self {
        match status {
            ApiWithdrawStatus::AuditReject => Self::AuditRejected,
            ApiWithdrawStatus::SendingTxFailed => match failure_stage {
                Some(WithdrawFailureStage::Build) => Self::SignFailed,
                Some(WithdrawFailureStage::Broadcast) => Self::BroadcastFailed,
                Some(WithdrawFailureStage::Chain) => Self::ChainFailed,
                // TxResultAck 阶段失败会自动重试，不算真正失败
                Some(WithdrawFailureStage::TxResultAck) => Self::UnknownFailed,
                _ => Self::UnknownFailed,
            },
            ApiWithdrawStatus::Failure => match failure_stage {
                Some(WithdrawFailureStage::Chain) => Self::ChainFailed,
                // TxResultAck 阶段失败会自动重试，不算真正失败
                Some(WithdrawFailureStage::TxResultAck) => Self::UnknownFailed,
                _ => match err_code {
                    Some(ErrCode::BalanceInsufficient) => Self::ResourceFailed,
                    Some(ErrCode::FeeInsufficient) => Self::ResourceFailed,
                    _ => Self::UnknownFailed,
                },
            },
            _ => Self::UnknownFailed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failure_reason_type_audit_rejected() {
        let reason = FailureReasonDisplay::from_status_and_error(
            ApiWithdrawStatus::AuditReject,
            None,
            None,
        );
        assert_eq!(reason, FailureReasonDisplay::AuditRejected);
        assert_eq!(serde_json::to_string(&reason).unwrap(), "1");
    }

    #[test]
    fn test_failure_reason_type_sign_failed() {
        let reason = FailureReasonDisplay::from_status_and_error(
            ApiWithdrawStatus::SendingTxFailed,
            None,
            Some(WithdrawFailureStage::Build),
        );
        assert_eq!(reason, FailureReasonDisplay::SignFailed);
        assert_eq!(serde_json::to_string(&reason).unwrap(), "2");
    }

    #[test]
    fn test_failure_reason_type_broadcast_failed() {
        let reason = FailureReasonDisplay::from_status_and_error(
            ApiWithdrawStatus::SendingTxFailed,
            None,
            Some(WithdrawFailureStage::Broadcast),
        );
        assert_eq!(reason, FailureReasonDisplay::BroadcastFailed);
        assert_eq!(serde_json::to_string(&reason).unwrap(), "3");
    }

    #[test]
    fn test_failure_reason_type_chain_failed() {
        let reason = FailureReasonDisplay::from_status_and_error(
            ApiWithdrawStatus::SendingTxFailed,
            None,
            Some(WithdrawFailureStage::Chain),
        );
        assert_eq!(reason, FailureReasonDisplay::ChainFailed);
        assert_eq!(serde_json::to_string(&reason).unwrap(), "4");
    }

    #[test]
    fn test_failure_reason_type_resource_failed() {
        let reason = FailureReasonDisplay::from_status_and_error(
            ApiWithdrawStatus::Failure,
            Some(ErrCode::BalanceInsufficient),
            None,
        );
        assert_eq!(reason, FailureReasonDisplay::ResourceFailed);
        assert_eq!(serde_json::to_string(&reason).unwrap(), "5");
    }

    #[test]
    fn test_failure_reason_type_tx_result_ack_returns_unknown() {
        // TxResultAck 阶段失败会自动重试，返回 UnknownFailed
        let reason = FailureReasonDisplay::from_status_and_error(
            ApiWithdrawStatus::SendingTxFailed,
            None,
            Some(WithdrawFailureStage::TxResultAck),
        );
        assert_eq!(reason, FailureReasonDisplay::UnknownFailed);
        assert_eq!(serde_json::to_string(&reason).unwrap(), "0");
    }

    #[test]
    fn test_failure_reason_type_unknown_failed() {
        let reason = FailureReasonDisplay::from_status_and_error(
            ApiWithdrawStatus::Failure,
            None,
            None,
        );
        assert_eq!(reason, FailureReasonDisplay::UnknownFailed);
        assert_eq!(serde_json::to_string(&reason).unwrap(), "0");
    }
}