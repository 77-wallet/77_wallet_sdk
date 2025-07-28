#[derive(Debug, Default, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ApiAccountEntity {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub address: String,
    pub pubkey: Option<String>,
    pub private_key: Option<String>,
    pub address_type: String,
    pub wallet_address: String,
    pub derivation_path: Option<String>,
    pub derivation_path_index: Option<String>,
    pub chain_code: String,
    pub wallet_type: String,
    pub status: i32,
    pub is_init: i32,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}
