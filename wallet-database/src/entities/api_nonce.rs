#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ApiNonceEntity {
    pub id: i64,
    pub uid: String,
    pub name: String,
    pub from_addr: String,
    pub chain_code: String,
    pub nonce: i32,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}
