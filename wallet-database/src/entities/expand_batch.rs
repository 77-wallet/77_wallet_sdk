#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ExpandBatchEntity {
    pub batch_id: String,
    pub uid: String,
    pub serial_no: String,
    pub chain_code: String,
    pub total_count: i32,
    pub finished_count: i32,
    pub status: ExpandBatchStatus, // 0=running, 1=done
    pub retry_count: i32,          // 重试次数
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

/// Running
///    |
///    | finished_count == total_count
///    v
/// Done
///    |
///    | expand_address_complete() 成功
///    v
/// Notified
#[derive(Debug, serde_repr::Serialize_repr, serde_repr::Deserialize_repr, sqlx::Type)]
#[repr(i32)]
pub enum ExpandBatchStatus {
    Running = 0,
    Done = 1,
    Notified = 2,
    Failed = 3,
}

#[derive(Debug, Clone)]
pub struct CreateExpandBatchEntity {
    pub uid: String,
    pub batch_id: String,
    pub serial_no: String,
    pub chain_code: String,
    pub total_count: i32,
}

impl CreateExpandBatchEntity {
    pub fn new(
        uid: &str,
        batch_id: &str,
        serial_no: &str,
        chain_code: &str,
        total_count: i32,
    ) -> Self {
        Self {
            uid: uid.to_string(),
            batch_id: batch_id.to_string(),
            serial_no: serial_no.to_string(),
            chain_code: chain_code.to_string(),
            total_count,
        }
    }
}

#[derive(sqlx::FromRow)]
pub struct BatchWithCount {
    #[sqlx(flatten)]
    pub batch: ExpandBatchEntity,
    pub item_count: i64,
}
