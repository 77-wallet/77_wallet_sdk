use crate::{
    application::api_wallet_resource::ApiResourceApplication,
    context::Context,
    error::service::ServiceError,
    request::api_wallet::resource::{
        ApiResourceStakeReq, ApiResourceUnstakeReq, ApiWithdrawWalletClaimVotesRewardsReq,
        ApiWithdrawWalletVoterInfoReq, ApiWithdrawWalletVotesNodeListReq,
        ApiWithdrawWalletVotesReq,
    },
    response_vo::{
        api_wallet::resource::ApiResourceOperationResp,
        standard_wallet::stake::{VoteListResp, VoterInfoResp},
    },
};

pub(crate) struct ApiResourceService {
    ctx: &'static Context,
}

impl ApiResourceService {
    pub fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub async fn stake_withdraw_wallet_resource(
        &self,
        req: ApiResourceStakeReq,
    ) -> Result<ApiResourceOperationResp, ServiceError> {
        ApiResourceApplication::new(self.ctx).stake_withdraw_wallet_resource(req).await
    }

    pub async fn unstake_withdraw_wallet_resource(
        &self,
        req: ApiResourceUnstakeReq,
    ) -> Result<ApiResourceOperationResp, ServiceError> {
        ApiResourceApplication::new(self.ctx).unstake_withdraw_wallet_resource(req).await
    }

    pub async fn withdraw_wallet_votes(
        &self,
        req: ApiWithdrawWalletVotesReq,
    ) -> Result<String, ServiceError> {
        ApiResourceApplication::new(self.ctx).withdraw_wallet_votes(req).await
    }

    pub async fn withdraw_wallet_voter_info(
        &self,
        req: ApiWithdrawWalletVoterInfoReq,
    ) -> Result<VoterInfoResp, ServiceError> {
        ApiResourceApplication::new(self.ctx).withdraw_wallet_voter_info(req).await
    }

    pub async fn withdraw_wallet_votes_node_list(
        &self,
        req: ApiWithdrawWalletVotesNodeListReq,
    ) -> Result<VoteListResp, ServiceError> {
        ApiResourceApplication::new(self.ctx).withdraw_wallet_votes_node_list(req).await
    }

    pub async fn withdraw_wallet_claim_votes_rewards(
        &self,
        req: ApiWithdrawWalletClaimVotesRewardsReq,
    ) -> Result<String, ServiceError> {
        ApiResourceApplication::new(self.ctx).withdraw_wallet_claim_votes_rewards(req).await
    }
}
