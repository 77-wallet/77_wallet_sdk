#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ExpandBatchEntity {
    pub batch_id: String,
    pub chain_code: String,
    pub total_count: i32,
    pub finished_count: i32,
    pub status: u8,             // 0=running, 1=done
    pub notified_complete: i32, // 0=未通知, 1=已通知
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct CreateExpandBatchEntity {
    pub batch_id: String,
    pub chain_code: String,
    pub total_count: i32,
}

impl CreateExpandBatchEntity {
    pub fn new(batch_id: &str, chain_code: &str, total_count: i32) -> Self {
        Self { batch_id: batch_id.to_string(), chain_code: chain_code.to_string(), total_count }
    }
}
