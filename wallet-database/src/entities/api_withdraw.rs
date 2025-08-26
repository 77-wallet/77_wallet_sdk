#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ApiWithdrawEntity {
    pub id: i64,
    pub name: String,
    pub uid: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
    pub chain_code: String,
    pub token_addr: Option<String>,
    pub symbol: String,
    pub trade_no: String,
    pub trade_type: u8,
    pub status: ApiWithdrawStatus,
    pub tx_hash: String,
    pub send_tx_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

#[derive(
    sqlx::Type, Debug, Clone, Copy, serde_repr::Deserialize_repr, serde_repr::Serialize_repr,
)]
#[repr(u8)]
pub enum ApiWithdrawStatus {
    Init,
    AuditPass,
    AuditReject,
}
