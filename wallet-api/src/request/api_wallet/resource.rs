use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ApiResourceType {
    Energy,
    Bandwidth,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResourceStakeReq {
    pub withdraw_wallet_uid: String,
    pub resource: ApiResourceType,
    pub frozen_balance: String,
    pub password: String,
}

impl fmt::Debug for ApiResourceStakeReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiResourceStakeReq")
            .field("withdraw_wallet_uid", &self.withdraw_wallet_uid)
            .field("resource", &self.resource)
            .field("frozen_balance", &self.frozen_balance)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResourceUnstakeReq {
    pub withdraw_wallet_uid: String,
    pub resource: ApiResourceType,
    pub unfreeze_balance: String,
    pub password: String,
}

impl fmt::Debug for ApiResourceUnstakeReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiResourceUnstakeReq")
            .field("withdraw_wallet_uid", &self.withdraw_wallet_uid)
            .field("resource", &self.resource)
            .field("unfreeze_balance", &self.unfreeze_balance)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiResourceStakeReq, ApiResourceType};

    #[test]
    fn api_resource_stake_req_debug_redacts_password() {
        let req = ApiResourceStakeReq {
            withdraw_wallet_uid: "withdraw".to_string(),
            resource: ApiResourceType::Energy,
            frozen_balance: "1000".to_string(),
            password: "secret".to_string(),
        };

        let debug = format!("{req:?}");
        assert!(!debug.contains("secret"));
        assert!(debug.contains("<redacted>"));
    }
}
