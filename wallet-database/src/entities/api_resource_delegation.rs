use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiResourceDelegationStatus {
    Pending = 1,
    Success = 2,
    Fail = 3,
}

impl ApiResourceDelegationStatus {
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiResourceDelegationResultStatus {
    Success = 1,
    Fail = 2,
}

impl ApiResourceDelegationResultStatus {
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiResourceDelegationEntity {
    pub id: i64,
    pub uid: String,
    pub source: String,
    pub operation_type: String,
    pub origin_trade_no: Option<String>,
    pub origin_trade_type: Option<i64>,
    pub resource_trade_no: String,
    pub chain_code: String,
    pub owner_address: String,
    pub receiver_address: String,
    pub resource_type: String,
    pub amount: String,
    pub status: i64,
    pub task_ack_sent_at: Option<DateTime<Utc>>,
    pub building_at: Option<DateTime<Utc>>,
    pub tx_hash: Option<String>,
    pub tx_status: Option<String>,
    pub tx_exec_receipt_uploaded_at: Option<DateTime<Utc>>,
    pub result_status: Option<i64>,
    pub result_received_at: Option<DateTime<Utc>>,
    pub result_ack_sent_at: Option<DateTime<Utc>>,
    pub result_payload: Option<String>,
    pub fail_type: Option<i64>,
    pub err_code: Option<String>,
    pub err_msg: Option<String>,
    pub recover_status: Option<String>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub retry_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewApiResourceDelegation {
    pub uid: String,
    pub source: String,
    pub operation_type: String,
    pub origin_trade_no: Option<String>,
    pub origin_trade_type: Option<i64>,
    pub resource_trade_no: String,
    pub chain_code: String,
    pub owner_address: String,
    pub receiver_address: String,
    pub resource_type: String,
    pub amount: String,
}

impl NewApiResourceDelegation {
    pub fn platform_delegate(
        uid: impl Into<String>,
        resource_trade_no: impl Into<String>,
        origin_trade_no: impl Into<String>,
        origin_trade_type: i64,
        owner_address: impl Into<String>,
        receiver_address: impl Into<String>,
        amount: impl Into<String>,
    ) -> Self {
        Self {
            uid: uid.into(),
            source: "platform".to_string(),
            operation_type: "delegate".to_string(),
            origin_trade_no: Some(origin_trade_no.into()),
            origin_trade_type: Some(origin_trade_type),
            resource_trade_no: resource_trade_no.into(),
            chain_code: "tron".to_string(),
            owner_address: owner_address.into(),
            receiver_address: receiver_address.into(),
            resource_type: "energy".to_string(),
            amount: amount.into(),
        }
    }
}
