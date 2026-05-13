use wallet_database::entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus};
use crate::response_vo::api_wallet::withdraw_display::FailureReasonDisplay;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWithdrawOrderVo {
    #[serde(flatten)]
    pub inner: ApiWithdrawEntity,
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
        let failure_reason_display = if matches!(entity.status, ApiWithdrawStatus::AuditReject | ApiWithdrawStatus::SendingTxFailed | ApiWithdrawStatus::Failure) {
            Some(FailureReasonDisplay::from_status_and_error(entity.status, entity.err_code, entity.failure_stage))
        } else {
            None
        };
        Self { apply_time: entity.created_at, sign_time, inner: entity, failure_reason_display }
    }
}