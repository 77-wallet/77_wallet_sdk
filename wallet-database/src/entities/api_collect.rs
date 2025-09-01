#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ApiCollectEntity {
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
    pub status: ApiCollectStatus,
    pub tx_hash: String,
    #[serde(skip_serializing)]
    pub resource_consume: String,
    pub transaction_fee: String,
    pub transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    pub block_height: String,
    pub notes: String,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

#[derive(
    sqlx::Type, Debug, Clone, Copy, serde_repr::Deserialize_repr, serde_repr::Serialize_repr,
)]
#[repr(u8)]
pub enum ApiCollectStatus {
    /// 初始化
    Init,
    /// 出款地址余额不足
    WithdrawInsufficientBalance,
    /// 发送交易
    SendingTx,
}
