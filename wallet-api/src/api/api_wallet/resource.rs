use crate::{
    api::ReturnType,
    manager::WalletManager,
    request::stake::{FreezeBalanceReq, UnFreezeBalanceReq, VoteWitnessReq, WithdrawBalanceReq},
    response_vo::standard_wallet::stake::{
        FreezeResp, VoteListResp, VoterInfoResp, WithdrawUnfreezeResp,
    },
    service::api_wallet::resource::ApiResourceService,
};

impl WalletManager {
    pub async fn stake_api_withdraw_wallet_resource(
        &self,
        req: FreezeBalanceReq,
        password: String,
    ) -> ReturnType<FreezeResp> {
        ApiResourceService::new(self.ctx).stake_withdraw_wallet_resource(req, password).await
    }

    pub async fn unstake_api_withdraw_wallet_resource(
        &self,
        req: UnFreezeBalanceReq,
        password: String,
    ) -> ReturnType<FreezeResp> {
        ApiResourceService::new(self.ctx).unstake_withdraw_wallet_resource(req, password).await
    }

    pub async fn api_withdraw_wallet_votes(
        &self,
        req: VoteWitnessReq,
        password: &str,
    ) -> ReturnType<String> {
        ApiResourceService::new(self.ctx).withdraw_wallet_votes(req, password).await
    }

    pub async fn api_withdraw_wallet_voter_info(
        &self,
        owner_address: &str,
    ) -> ReturnType<VoterInfoResp> {
        ApiResourceService::new(self.ctx).withdraw_wallet_voter_info(owner_address).await
    }

    pub async fn api_withdraw_wallet_votes_node_list(
        &self,
        owner_address: Option<&str>,
    ) -> ReturnType<VoteListResp> {
        ApiResourceService::new(self.ctx).withdraw_wallet_votes_node_list(owner_address).await
    }

    pub async fn api_withdraw_wallet_claim_votes_rewards(
        &self,
        req: WithdrawBalanceReq,
        password: &str,
    ) -> ReturnType<String> {
        ApiResourceService::new(self.ctx).withdraw_wallet_claim_votes_rewards(req, password).await
    }

    pub async fn api_withdraw_wallet_unfreeze(
        &self,
        req: WithdrawBalanceReq,
        password: String,
    ) -> ReturnType<WithdrawUnfreezeResp> {
        ApiResourceService::new(self.ctx).withdraw_wallet_unfreeze(req, password).await
    }
}
