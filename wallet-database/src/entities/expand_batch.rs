use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ExpandBatchEntity {
    pub batch_id: String,
    pub chain_code: String,
    pub total_count: i32,
    pub finished_count: i32,
    pub status: u8,              // 0=running, 1=done
    pub created_at: i64,         // 时间戳
    pub updated_at: Option<i64>, // 时间戳
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

    pub fn with_current_time(self) -> (Self, i64) {
        let timestamp =
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64;
        (self, timestamp)
    }
}
