use std::fmt::Display;
#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiFeeEntity {
    pub id: i64,
    pub name: String,
    pub uid: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
    pub validate: String,
    pub chain_code: String,
    pub token_addr: Option<String>,
    pub symbol: String,
    pub trade_no: String,
    pub trade_type: u8,
    pub status: ApiFeeStatus,
    pub tx_hash: String,
    #[serde(skip_serializing)]
    pub resource_consume: String,
    pub transaction_fee: String,
    pub transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    pub block_height: String,
    pub notes: String,
    pub post_tx_count: u32,
    pub post_confirm_tx_count: u32,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

#[derive(
    sqlx::Type,
    Debug,
    Clone,
    Copy,
    serde_repr::Deserialize_repr,
    serde_repr::Serialize_repr,
    PartialEq,
)]
#[repr(u8)]
pub enum ApiFeeStatus {
    Init,                  // 0
    SendingTx,             // 1
    SendingTxFailed,       // 2
    SendingTxReport,       // 3
    SendingTxFailedReport, //4, 结束
    Success,               // 5,
    Failure,               // 6
    ConfirmSuccessReport,  // 7, 结束
    ConfirmFailureReport,  // 8, 结束
}

impl Display for ApiFeeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", *self as u8)
    }
}
