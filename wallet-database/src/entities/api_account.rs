use crate::entities::api_wallet::ApiWalletType;

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiAccountEntity {
    pub id: i64,
    pub account_id: u32,
    pub name: String,
    pub address: String,
    pub pubkey: Option<String>,
    pub address_type: String,
    pub wallet_address: String,
    pub uid: String,
    pub derivation_path: String,
    pub derivation_path_index: i32,
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
#[derive(Clone)]
pub struct CreateApiAccountVo {
    pub account_id: u32,
    pub address: String,
    pub pubkey: String,
    pub address_type: String,
    pub wallet_address: String,
    pub uid: String,
    pub derivation_path: String,
    pub derivation_path_index: i32,
    pub chain_code: String,
    pub name: String,
    pub api_wallet_type: ApiWalletType,
    pub is_init: i32,
}

impl CreateApiAccountVo {
    pub fn new(
        account_id: u32,
        address: &str,
        pubkey: &str,
        wallet_address: &str,
        uid: &str,
        derivation_path: &str,
        derivation_path_index: i32,
        chain_code: &str,
        name: &str,
        api_wallet_type: ApiWalletType,
    ) -> Self {
        Self {
            account_id,
            address: address.to_string(),
            pubkey: pubkey.to_string(),
            address_type: "".to_string(),
            wallet_address: wallet_address.to_string(),
            uid: uid.to_string(),
            derivation_path: derivation_path.to_string(),
            derivation_path_index,
            chain_code: chain_code.to_string(),
            name: name.to_string(),
            api_wallet_type,
            is_init: 0,
        }
    }

    pub fn with_address_type(mut self, address_type: &str) -> Self {
        self.address_type = address_type.to_string();
        self
    }

    pub fn with_is_init(mut self, is_init: bool) -> Self {
        self.is_init = is_init as i32;
        self
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AccountToWalletAddress {
    pub address: String,
    pub wallet_address: String,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiAccountWalletMapping {
    pub account_id: u32,
    #[sqlx(rename = "name")]
    pub account_name: String,
    pub address: String,
    pub wallet_address: String,
    pub seed: String,
    pub uid: String,
    pub api_wallet_type: ApiWalletType,
}
