use crate::{
    application::api_wallet_resource::ApiResourceApplication,
    context::Context,
    error::service::ServiceError,
    request::stake::{FreezeBalanceReq, UnFreezeBalanceReq, VoteWitnessReq, WithdrawBalanceReq},
    response_vo::standard_wallet::stake::{
        FreezeResp, VoteListResp, VoterInfoResp, WithdrawUnfreezeResp,
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
        req: FreezeBalanceReq,
        password: String,
    ) -> Result<FreezeResp, ServiceError> {
        ApiResourceApplication::new(self.ctx).stake_withdraw_wallet_resource(req, password).await
    }

    pub async fn unstake_withdraw_wallet_resource(
        &self,
        req: UnFreezeBalanceReq,
        password: String,
    ) -> Result<FreezeResp, ServiceError> {
        ApiResourceApplication::new(self.ctx).unstake_withdraw_wallet_resource(req, password).await
    }

    pub async fn withdraw_wallet_votes(
        &self,
        req: VoteWitnessReq,
        password: &str,
    ) -> Result<String, ServiceError> {
        ApiResourceApplication::new(self.ctx).withdraw_wallet_votes(req, password).await
    }

    pub async fn withdraw_wallet_voter_info(
        &self,
        owner_address: &str,
    ) -> Result<VoterInfoResp, ServiceError> {
        ApiResourceApplication::new(self.ctx).withdraw_wallet_voter_info(owner_address).await
    }

    pub async fn withdraw_wallet_votes_node_list(
        &self,
        owner_address: Option<&str>,
    ) -> Result<VoteListResp, ServiceError> {
        ApiResourceApplication::new(self.ctx).withdraw_wallet_votes_node_list(owner_address).await
    }

    pub async fn withdraw_wallet_claim_votes_rewards(
        &self,
        req: WithdrawBalanceReq,
        password: &str,
    ) -> Result<String, ServiceError> {
        ApiResourceApplication::new(self.ctx)
            .withdraw_wallet_claim_votes_rewards(req, password)
            .await
    }

    pub async fn withdraw_wallet_unfreeze(
        &self,
        req: WithdrawBalanceReq,
        password: String,
    ) -> Result<WithdrawUnfreezeResp, ServiceError> {
        ApiResourceApplication::new(self.ctx).withdraw_wallet_unfreeze(req, password).await
    }
}
