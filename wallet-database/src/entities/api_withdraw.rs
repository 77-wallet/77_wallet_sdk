use std::fmt::Display;
#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ApiWithdrawEntity {
    #[serde(skip_serializing)]
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
    #[serde(skip_serializing)]
    pub resource_consume: String,
    pub transaction_fee: String,
    pub transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    pub block_height: String,
    pub notes: String,
    pub post_tx_count: u32,
    pub post_confirm_tx_count: u32,
    #[serde(skip_serializing)]
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    #[serde(skip_serializing)]
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
    Eq,
)]
#[repr(u8)]
pub enum ApiWithdrawStatus {
    Init, // 0
    AuditPass, // 1
    AuditReject, // 2
    SendingTx, // 3
    SendingTxFailed, // 4
    SendingTxReport, // 5, 发送交易报告给服务器
    SendingTxFailedReport, // 6,发送交易失败报告给服务器，结束
    Success, // 7，收到成功确认
    Failure, // 8，收到失败确认
    ReceivedConfirmReport, // 9, 结束
}

impl Display for ApiWithdrawStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", *self as u8)
    }
}
