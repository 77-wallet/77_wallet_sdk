use wallet_database::entities::asset_token_key::AssetTokenKey;

pub struct ServiceFeePayer {
    pub from: String,
    pub chain_code: String,
    pub symbol: String,
    pub fee_setting: Option<String>,
    pub request_resource_id: Option<String>,
    pub token_address: Option<String>,
}

impl ServiceFeePayer {
    pub fn token_key(&self) -> AssetTokenKey {
        AssetTokenKey::from_raw(self.token_address.as_deref())
    }
}

pub struct DeployFeePayer {
    pub account_id: String,
    pub fee_setting: String,
}

pub struct Executor {
    pub address: String,
    pub password: String,
}
