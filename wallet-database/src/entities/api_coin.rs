use chrono::{DateTime, Utc};

#[derive(Debug, Default, serde::Serialize, sqlx::FromRow)]
pub struct ApiCoin {
    pub id: i64,
    pub name: Option<String>,
    pub symbol: String,
    pub chain_code: String,
    pub token_address: Option<String>,
    pub price: Option<String>,
    pub protocol: Option<String>,
    pub decimals: u8,
    pub is_default: u8,
    pub is_popular: u8,
    pub is_custom: u8,
    pub status: u8,
    // 是否支持兑换
    pub swappable: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
