use crate::{
    api::ReturnType,
    manager::WalletManager,
    request::api_wallet::resource::{
        ApiResourceStakeReq, ApiResourceUnstakeReq, ApiWithdrawWalletClaimVotesRewardsReq,
        ApiWithdrawWalletVoterInfoReq, ApiWithdrawWalletVotesNodeListReq,
        ApiWithdrawWalletVotesReq,
    },
    response_vo::{
        api_wallet::resource::ApiResourceOperationResp,
        standard_wallet::stake::{VoteListResp, VoterInfoResp},
    },
    service::api_wallet::resource::ApiResourceService,
};

impl WalletManager {
    pub async fn stake_api_withdraw_wallet_resource(
        &self,
        req: ApiResourceStakeReq,
    ) -> ReturnType<ApiResourceOperationResp> {
        ApiResourceService::new(self.ctx).stake_withdraw_wallet_resource(req).await
    }

    pub async fn unstake_api_withdraw_wallet_resource(
        &self,
        req: ApiResourceUnstakeReq,
    ) -> ReturnType<ApiResourceOperationResp> {
        ApiResourceService::new(self.ctx).unstake_withdraw_wallet_resource(req).await
    }

    pub async fn api_withdraw_wallet_votes(
        &self,
        req: ApiWithdrawWalletVotesReq,
    ) -> ReturnType<String> {
        ApiResourceService::new(self.ctx).withdraw_wallet_votes(req).await
    }

    pub async fn api_withdraw_wallet_voter_info(
        &self,
        req: ApiWithdrawWalletVoterInfoReq,
    ) -> ReturnType<VoterInfoResp> {
        ApiResourceService::new(self.ctx).withdraw_wallet_voter_info(req).await
    }

    pub async fn api_withdraw_wallet_votes_node_list(
        &self,
        req: ApiWithdrawWalletVotesNodeListReq,
    ) -> ReturnType<VoteListResp> {
        ApiResourceService::new(self.ctx).withdraw_wallet_votes_node_list(req).await
    }

    pub async fn api_withdraw_wallet_claim_votes_rewards(
        &self,
        req: ApiWithdrawWalletClaimVotesRewardsReq,
    ) -> ReturnType<String> {
        ApiResourceService::new(self.ctx).withdraw_wallet_claim_votes_rewards(req).await
    }
}
