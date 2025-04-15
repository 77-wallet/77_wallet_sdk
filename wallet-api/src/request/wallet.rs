pub struct ResetRootReq {
    pub language_code: u8,
    pub phrase: String,
    pub salt: String,
    pub wallet_address: String,
    pub new_password: String,
    pub subkey_password: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateWalletReq {
    pub language_code: u8,
    pub phrase: String,
    pub salt: String,
    pub wallet_name: String,
    pub account_name: String,
    pub is_default_name: bool,
    pub wallet_password: String,
    pub derive_password: Option<String>,
    // 邀请码
    pub invite_code: Option<String>,
}

impl CreateWalletReq {
    pub fn new(
        language_code: u8,
        phrase: &str,
        salt: &str,
        wallet_name: &str,
        account_name: &str,
        is_default_name: bool,
        wallet_password: &str,
        derive_password: Option<String>,
        invite_code: Option<String>,
    ) -> Self {
        Self {
            language_code,
            phrase: phrase.to_string(),
            salt: salt.to_string(),
            wallet_name: wallet_name.to_string(),
            account_name: account_name.to_string(),
            is_default_name,
            wallet_password: wallet_password.to_string(),
            derive_password,
            invite_code,
        }
    }
}
