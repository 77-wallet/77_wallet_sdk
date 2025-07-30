use wallet_database::entities::api_wallet::ApiWalletType;

#[derive(Debug, serde::Deserialize, Clone)]
pub struct CreateApiAccountReq {
    pub wallet_address: String,
    pub wallet_password: String,
    pub index: Option<i32>,
    pub name: String,
    pub is_default_name: bool,
    pub api_wallet_type: ApiWalletType,
}

impl CreateApiAccountReq {
    pub fn new(
        wallet_address: &str,
        wallet_password: &str,
        index: Option<i32>,
        name: &str,
        is_default_name: bool,
        api_wallet_type: ApiWalletType,
    ) -> Self {
        Self {
            wallet_address: wallet_address.to_string(),
            wallet_password: wallet_password.to_string(),
            index,
            name: name.to_string(),
            is_default_name,
            api_wallet_type,
        }
    }
}
