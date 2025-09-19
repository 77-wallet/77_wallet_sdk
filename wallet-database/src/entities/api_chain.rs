use crate::entities::chain::StringList;

#[derive(Debug, Default, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiChainEntity {
    pub id: i64,
    pub name: String,
    pub chain_code: String,
    pub main_symbol: String,
    pub node_id: Option<String>,
    // #[sqlx(type_name = "TEXT")]
    pub protocols: StringList,
    pub status: u8,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}
