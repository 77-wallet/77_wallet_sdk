use crate::{
    application::wallet::WalletApplication,
    context::Context,
    domain::api_wallet::resource::ApiResourceDomain,
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
    service::stake::StackService,
};
use wallet_database::{
    entities::{
        api_resource_operation::{ApiResourceOperationType, NewApiResourceOperation},
        api_resource_type::ApiResourceType as DbApiResourceType,
    },
    repositories::api_wallet::resource_operation::ApiResourceOperationRepo,
};

pub(crate) struct ApiResourceApplication {
    ctx: &'static Context,
}

impl ApiResourceApplication {
    pub(crate) fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub(crate) async fn stake_withdraw_wallet_resource(
        &self,
        req: ApiResourceStakeReq,
    ) -> Result<ApiResourceOperationResp, ServiceError> {
        WalletApplication::validate_password(&req.password).await?;

        let outcome = ApiResourceDomain::stake_withdraw_wallet_resource(
            self.ctx,
            &req.withdraw_wallet_uid,
            req.resource,
            &req.frozen_balance,
        )
        .await?;

        let resource_trade_no = Self::client_resource_trade_no();
        self.record_client_resource_operation(
            &req.withdraw_wallet_uid,
            &resource_trade_no,
            outcome.owner_address,
            req.resource,
            req.frozen_balance,
            ApiResourceOperationType::Stake,
            &outcome.tx_hash,
            &outcome.raw_tx,
            &outcome.transaction_fee,
        )
        .await?;

        Ok(ApiResourceOperationResp::success(resource_trade_no, outcome.tx_hash))
    }

    pub(crate) async fn unstake_withdraw_wallet_resource(
        &self,
        req: ApiResourceUnstakeReq,
    ) -> Result<ApiResourceOperationResp, ServiceError> {
        WalletApplication::validate_password(&req.password).await?;

        let outcome = ApiResourceDomain::unstake_withdraw_wallet_resource(
            self.ctx,
            &req.withdraw_wallet_uid,
            req.resource,
            &req.unfreeze_balance,
        )
        .await?;

        let resource_trade_no = Self::client_resource_trade_no();
        self.record_client_resource_operation(
            &req.withdraw_wallet_uid,
            &resource_trade_no,
            outcome.owner_address,
            req.resource,
            req.unfreeze_balance,
            ApiResourceOperationType::Unstake,
            &outcome.tx_hash,
            &outcome.raw_tx,
            &outcome.transaction_fee,
        )
        .await?;

        Ok(ApiResourceOperationResp::success(resource_trade_no, outcome.tx_hash))
    }

    pub(crate) async fn withdraw_wallet_votes(
        &self,
        req: ApiWithdrawWalletVotesReq,
    ) -> Result<String, ServiceError> {
        WalletApplication::validate_password(&req.password).await?;
        let owner_address =
            ApiResourceDomain::withdraw_wallet_address(self.ctx, &req.withdraw_wallet_uid).await?;
        let vote_req = ApiResourceDomain::votes_req_for_withdraw_wallet(owner_address, req.votes);
        StackService::new().await?.votes(vote_req, &req.password).await
    }

    pub(crate) async fn withdraw_wallet_voter_info(
        &self,
        req: ApiWithdrawWalletVoterInfoReq,
    ) -> Result<VoterInfoResp, ServiceError> {
        let owner_address =
            ApiResourceDomain::withdraw_wallet_address(self.ctx, &req.withdraw_wallet_uid).await?;
        StackService::new().await?.voter_info(&owner_address).await
    }

    pub(crate) async fn withdraw_wallet_votes_node_list(
        &self,
        req: ApiWithdrawWalletVotesNodeListReq,
    ) -> Result<VoteListResp, ServiceError> {
        let owner_address =
            ApiResourceDomain::withdraw_wallet_address(self.ctx, &req.withdraw_wallet_uid).await?;
        StackService::new().await?.vote_list(Some(&owner_address)).await
    }

    pub(crate) async fn withdraw_wallet_claim_votes_rewards(
        &self,
        req: ApiWithdrawWalletClaimVotesRewardsReq,
    ) -> Result<String, ServiceError> {
        WalletApplication::validate_password(&req.password).await?;
        let owner_address =
            ApiResourceDomain::withdraw_wallet_address(self.ctx, &req.withdraw_wallet_uid).await?;
        let claim_req =
            ApiResourceDomain::claim_votes_rewards_req_for_withdraw_wallet(owner_address);
        StackService::new().await?.votes_claim_rewards(claim_req, &req.password).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_client_resource_operation(
        &self,
        uid: &str,
        resource_trade_no: &str,
        owner_address: String,
        resource: crate::request::api_wallet::resource::ApiResourceType,
        amount: String,
        operation_type: ApiResourceOperationType,
        tx_hash: &str,
        raw_tx: &str,
        transaction_fee: &str,
    ) -> Result<(), ServiceError> {
        let pool = self.ctx.api_transaction_pool()?;
        let input = NewApiResourceOperation::client(
            uid,
            resource_trade_no,
            owner_address,
            Self::db_resource_type(resource),
            amount,
            operation_type,
        );
        ApiResourceOperationRepo::record_client_broadcast_success(
            &pool,
            input,
            tx_hash,
            raw_tx,
            transaction_fee,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))
    }

    fn db_resource_type(
        resource: crate::request::api_wallet::resource::ApiResourceType,
    ) -> DbApiResourceType {
        match resource {
            crate::request::api_wallet::resource::ApiResourceType::Energy => {
                DbApiResourceType::Energy
            }
            crate::request::api_wallet::resource::ApiResourceType::Bandwidth => {
                DbApiResourceType::Bandwidth
            }
        }
    }

    fn client_resource_trade_no() -> String {
        format!("client_rsc_{}", uuid::Uuid::new_v4().simple())
    }
}
