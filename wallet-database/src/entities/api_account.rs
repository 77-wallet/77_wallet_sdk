use crate::entities::api_wallet::ApiWalletType;

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ApiAccountEntity {
    pub id: i64,
    pub account_id: u32,
    pub name: String,
    pub address: String,
    pub pubkey: Option<String>,
    pub private_key: String,
    pub address_type: String,
    pub wallet_address: String,
    pub derivation_path: String,
    pub derivation_path_index: Option<String>,
    pub chain_code: String,
    pub api_wallet_type: ApiWalletType,
    pub status: i32,
    pub is_init: i32,
    pub is_expand: i32,
    pub is_used: bool,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

impl ApiAccountEntity {
    pub fn address_type(&self) -> Option<String> {
        (!self.address_type.is_empty()).then(|| self.address_type.clone())
    }
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct CreateApiAccountVo {
    pub account_id: u32,
    pub address: String,
    pub pubkey: String,
    #[serde(skip_serializing)]
    pub private_key: String,
    pub address_type: String,
    pub wallet_address: String,
    pub derivation_path: String,
    pub derivation_path_index: Option<String>,
    pub chain_code: String,
    pub name: String,
    pub api_wallet_type: ApiWalletType,
}

impl CreateApiAccountVo {
    pub fn new(
        account_id: u32,
        address: &str,
        pubkey: &str,
        private_key: &str,
        wallet_address: &str,
        derivation_path: &str,
        derivation_path_index: &str,
        chain_code: &str,
        name: &str,
        api_wallet_type: ApiWalletType,
    ) -> Self {
        Self {
            account_id,
            address: address.to_string(),
            pubkey: pubkey.to_string(),
            private_key: private_key.to_string(),
            address_type: "".to_string(),
            wallet_address: wallet_address.to_string(),
            derivation_path: derivation_path.to_string(),
            derivation_path_index: Some(derivation_path_index.to_string()),
            chain_code: chain_code.to_string(),
            name: name.to_string(),
            api_wallet_type,
        }
    }

    pub fn with_address_type(mut self, address_type: &str) -> Self {
        self.address_type = address_type.to_string();
        self
    }
}
