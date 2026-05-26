use std::fmt;

use crate::request::stake::VotesReq;

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWithdrawWalletVotesNodeListReq {
    pub withdraw_wallet_uid: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWithdrawWalletVoterInfoReq {
    pub withdraw_wallet_uid: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWithdrawWalletVotesReq {
    pub withdraw_wallet_uid: String,
    pub votes: Vec<VotesReq>,
    pub password: String,
}

impl fmt::Debug for ApiWithdrawWalletVotesReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiWithdrawWalletVotesReq")
            .field("withdraw_wallet_uid", &self.withdraw_wallet_uid)
            .field("votes", &self.votes)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWithdrawWalletClaimVotesRewardsReq {
    pub withdraw_wallet_uid: String,
    pub password: String,
}

impl fmt::Debug for ApiWithdrawWalletClaimVotesRewardsReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiWithdrawWalletClaimVotesRewardsReq")
            .field("withdraw_wallet_uid", &self.withdraw_wallet_uid)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApiResourceStakeReq, ApiResourceType, ApiWithdrawWalletClaimVotesRewardsReq,
        ApiWithdrawWalletVotesReq,
    };
    use crate::request::stake::VotesReq;

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

    #[test]
    fn api_resource_votes_req_debug_redacts_password() {
        let req = ApiWithdrawWalletVotesReq {
            withdraw_wallet_uid: "withdraw".to_string(),
            votes: vec![VotesReq::new("TNode", 1, "node")],
            password: "secret".to_string(),
        };

        let debug = format!("{req:?}");
        assert!(!debug.contains("secret"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn api_resource_claim_votes_rewards_req_debug_redacts_password() {
        let req = ApiWithdrawWalletClaimVotesRewardsReq {
            withdraw_wallet_uid: "withdraw".to_string(),
            password: "secret".to_string(),
        };

        let debug = format!("{req:?}");
        assert!(!debug.contains("secret"));
        assert!(debug.contains("<redacted>"));
    }
}
