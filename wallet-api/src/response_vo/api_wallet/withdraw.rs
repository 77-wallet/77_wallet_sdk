use crate::response_vo::api_wallet::withdraw_display::FailureReasonDisplay;
use wallet_database::entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus, ErrCode};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWithdrawOrderVo {
    pub trade_no: String,
    pub chain_code: String,
    pub symbol: String,
    pub value: String,
    pub from_addr: String,
    pub to_addr: String,
    pub status: ApiWithdrawStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub audit_passed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub audit_rejected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub err_msg: Option<String>,
    pub tx_hash: Option<String>,
    /// 申请时间，派生自 created_at
    pub apply_time: chrono::DateTime<chrono::Utc>,
    /// 签名时间，派生自 audit_passed_at ?? audit_rejected_at
    /// 通过单展示 audit_passed_at，拒绝单展示 audit_rejected_at
    pub sign_time: Option<chrono::DateTime<chrono::Utc>>,
    /// 失败原因类型标识（仅在失败状态时返回，供前端映射文案使用）
    pub failure_reason_display: Option<FailureReasonDisplay>,
}

impl From<ApiWithdrawEntity> for ApiWithdrawOrderVo {
    fn from(entity: ApiWithdrawEntity) -> Self {
        let sign_time = entity.audit_passed_at.or(entity.audit_rejected_at);
        let failure_reason_display =
            failure_reason_display(entity.status, entity.err_code, entity.failure_stage);
        Self {
            trade_no: entity.trade_no,
            chain_code: entity.chain_code,
            symbol: entity.symbol,
            value: entity.value,
            from_addr: entity.from_addr,
            to_addr: entity.to_addr,
            status: entity.status,
            created_at: entity.created_at,
            audit_passed_at: entity.audit_passed_at,
            audit_rejected_at: entity.audit_rejected_at,
            err_msg: entity.err_msg,
            tx_hash: entity.tx_hash,
            apply_time: entity.created_at,
            sign_time,
            failure_reason_display,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWithdrawOrderDetailVo {
    pub trade_no: String,
    pub chain_code: String,
    pub symbol: String,
    pub value: String,
    pub from_addr: String,
    pub to_addr: String,
    pub validate: String,
    pub status: ApiWithdrawStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub audit_passed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub audit_rejected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub audit_reason: Option<String>,
    pub tx_hash: Option<String>,
    pub err_code: Option<ErrCode>,
    pub err_msg: Option<String>,
    pub notes: Option<String>,
    /// 申请时间，派生自 created_at
    pub apply_time: chrono::DateTime<chrono::Utc>,
    /// 签名时间，派生自 audit_passed_at ?? audit_rejected_at
    pub sign_time: Option<chrono::DateTime<chrono::Utc>>,
    /// 失败原因类型标识（仅在失败状态时返回，供前端映射文案使用）
    pub failure_reason_display: Option<FailureReasonDisplay>,
}

impl From<ApiWithdrawEntity> for ApiWithdrawOrderDetailVo {
    fn from(entity: ApiWithdrawEntity) -> Self {
        let sign_time = entity.audit_passed_at.or(entity.audit_rejected_at);
        let failure_reason_display =
            failure_reason_display(entity.status, entity.err_code, entity.failure_stage);
        Self {
            trade_no: entity.trade_no,
            chain_code: entity.chain_code,
            symbol: entity.symbol,
            value: entity.value,
            from_addr: entity.from_addr,
            to_addr: entity.to_addr,
            validate: entity.validate,
            status: entity.status,
            created_at: entity.created_at,
            audit_passed_at: entity.audit_passed_at,
            audit_rejected_at: entity.audit_rejected_at,
            audit_reason: entity.audit_reason,
            tx_hash: entity.tx_hash,
            err_code: entity.err_code,
            err_msg: entity.err_msg,
            notes: entity.notes,
            apply_time: entity.created_at,
            sign_time,
            failure_reason_display,
        }
    }
}

fn failure_reason_display(
    status: ApiWithdrawStatus,
    err_code: Option<ErrCode>,
    failure_stage: Option<wallet_database::entities::api_withdraw::WithdrawFailureStage>,
) -> Option<FailureReasonDisplay> {
    if matches!(
        status,
        ApiWithdrawStatus::AuditReject
            | ApiWithdrawStatus::SendingTxFailed
            | ApiWithdrawStatus::Failure
    ) {
        Some(FailureReasonDisplay::from_status_and_error(status, err_code, failure_stage))
    } else {
        None
    }
}
