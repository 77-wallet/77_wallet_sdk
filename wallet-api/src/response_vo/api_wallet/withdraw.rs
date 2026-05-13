use wallet_database::entities::api_withdraw::ApiWithdrawEntity;

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
}

impl From<ApiWithdrawEntity> for ApiWithdrawOrderVo {
    fn from(entity: ApiWithdrawEntity) -> Self {
        let sign_time = entity.audit_passed_at.or(entity.audit_rejected_at);
        Self {
            apply_time: entity.created_at,
            sign_time,
            inner: entity,
        }
    }
}