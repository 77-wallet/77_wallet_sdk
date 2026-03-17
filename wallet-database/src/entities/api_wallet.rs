#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ApiWalletEntity {
    pub id: i64,
    pub name: String,
    pub uid: String,
    pub address: String,
    #[serde(skip_serializing)]
    pub phrase: String,
    #[serde(skip_serializing)]
    pub seed: String,
    pub binding_address: Option<String>,
    pub api_wallet_type: ApiWalletType,
    pub merchant_id: Option<String>,
    pub app_id: Option<String>,
    pub sn: Option<String>,
    pub status: u8,
    pub is_init: u16,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    serde_repr::Deserialize_repr,
    serde_repr::Serialize_repr,
    sqlx::Type,
    PartialEq,
)]
#[repr(u8)]
pub enum ApiWalletType {
    SubAccount = 1,
    Withdrawal = 2,
}

impl TryFrom<u8> for ApiWalletType {
    type Error = crate::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ApiWalletType::SubAccount),
            2 => Ok(ApiWalletType::Withdrawal),
            _ => Err(crate::Error::InvalidValue(value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ApiWalletType;

    #[test]
    fn api_wallet_type_try_from_accepts_only_1_and_2() {
        assert!(matches!(ApiWalletType::try_from(1), Ok(ApiWalletType::SubAccount)));
        assert!(matches!(ApiWalletType::try_from(2), Ok(ApiWalletType::Withdrawal)));
        assert!(ApiWalletType::try_from(0).is_err());
        assert!(ApiWalletType::try_from(3).is_err());
    }
}
