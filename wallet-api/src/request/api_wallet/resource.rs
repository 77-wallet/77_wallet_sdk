use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ApiResourceType {
    Energy,
    Bandwidth,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResourceStakeReq {
    pub wallet_id: String,
    pub withdraw_wallet_uid: String,
    pub resource_type: ApiResourceType,
    pub amount: String,
    pub password: String,
}

impl fmt::Debug for ApiResourceStakeReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiResourceStakeReq")
            .field("wallet_id", &self.wallet_id)
            .field("withdraw_wallet_uid", &self.withdraw_wallet_uid)
            .field("resource_type", &self.resource_type)
            .field("amount", &self.amount)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResourceUnstakeReq {
    pub wallet_id: String,
    pub withdraw_wallet_uid: String,
    pub resource_type: ApiResourceType,
    pub amount: String,
    pub password: String,
}

impl fmt::Debug for ApiResourceUnstakeReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiResourceUnstakeReq")
            .field("wallet_id", &self.wallet_id)
            .field("withdraw_wallet_uid", &self.withdraw_wallet_uid)
            .field("resource_type", &self.resource_type)
            .field("amount", &self.amount)
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
            wallet_id: "wallet".to_string(),
            withdraw_wallet_uid: "withdraw".to_string(),
            resource_type: ApiResourceType::Energy,
            amount: "1000".to_string(),
            password: "secret".to_string(),
        };

        let debug = format!("{req:?}");
        assert!(!debug.contains("secret"));
        assert!(debug.contains("<redacted>"));
    }
}
