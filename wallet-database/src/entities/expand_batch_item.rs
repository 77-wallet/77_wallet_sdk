#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ExpandBatchItemEntity {
    pub batch_id: String,
    pub uid: String,
    pub chain_code: String,
    pub input_index: i32,
    pub status: u8, // 0=initing, 1=done
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct CreateExpandBatchItemEntity {
    pub batch_id: String,
    pub uid: String,
    pub chain_code: String,
    pub input_index: i32,
}

impl CreateExpandBatchItemEntity {
    pub fn new(batch_id: &str, uid: &str, chain_code: &str, input_index: i32) -> Self {
        Self {
            batch_id: batch_id.to_string(),
            uid: uid.to_string(),
            chain_code: chain_code.to_string(),
            input_index,
        }
    }
}
