#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ApiWithdrawStrategyEntity {
    pub id: i64,
    pub name: String,
    pub uid: String,
    pub min_value: String,
    pub idx: i32,
    pub risk_idx: i32,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}
