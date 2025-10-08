use std::fmt::Display;
use crate::entities::api_withdraw::ApiWithdrawStatus;

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
pub enum ApiCollectStatus {
    Init, // 0, 初始化
    InsufficientBalance,
    SufficientBalance,
    /// 1, 出款地址余额不足
    SendingTx,
    ///2, 发送交易
    SendingTxFailed, // 3
    SendingTxReport,       // 4, 发送交易报告给服务器
    SendingTxFailedReport, // 5,发送交易失败报告给服务器，结束
    Success,               // 6，收到成功确认
    Failure,               // 7，收到失败确认
    ReceivedConfirmReport, // 8, 结束
}

impl Display for ApiCollectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", *self as u8)
    }
}
