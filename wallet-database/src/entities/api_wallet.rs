#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ApiWalletEntity {
    pub id: i64,
    pub name: String,
    pub uid: String,
    pub address: String,
    pub phrase: String,
    pub seed: String,
    pub wallet_type: ApiWalletType,
    pub merchant_id: String,
    pub app_id: String,
    pub status: u8,
    pub is_init: u16,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

#[derive(
    Debug, Clone, Copy, serde_repr::Deserialize_repr, serde_repr::Serialize_repr, sqlx::Type, PartialEq
)]
#[repr(u8)]
pub enum ApiWalletType {
    InvalidValue = 0,
    SubAccount = 1,
    Withdrawal = 2,
}

impl From<u8> for ApiWalletType {
    fn from(value: u8) -> Self {
        match value {
            1 => ApiWalletType::SubAccount,
            2 => ApiWalletType::Withdrawal,
            _ => ApiWalletType::InvalidValue,
        }
    }
}
