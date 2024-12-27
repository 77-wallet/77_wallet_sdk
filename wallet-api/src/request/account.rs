#[derive(Debug, serde::Deserialize, Clone)]
pub struct CreateAccountReq {
    pub wallet_address: String,
    pub root_password: String,
    pub derive_password: Option<String>,
    pub derivation_path: Option<String>,
    pub index: Option<i32>,
    pub name: String,
    pub is_default_name: bool,
}

impl CreateAccountReq {
    pub fn new(
        wallet_address: &str,
        root_password: &str,
        derive_password: Option<String>,
        derivation_path: Option<String>,
        index: Option<i32>,
        name: &str,
        is_default_name: bool,
    ) -> Self {
        Self {
            wallet_address: wallet_address.to_string(),
            root_password: root_password.to_string(),
            derive_password,
            derivation_path,
            index,
            name: name.to_string(),
            is_default_name,
        }
    }
}
