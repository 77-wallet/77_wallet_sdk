use crate::{
    application::wallet::WalletApplication,
    context::Context,
    domain::api_wallet::resource::ApiResourceDomain,
    error::service::ServiceError,
    request::stake::{FreezeBalanceReq, UnFreezeBalanceReq, VoteWitnessReq, WithdrawBalanceReq},
    response_vo::standard_wallet::stake::{FreezeResp, VoteListResp, VoterInfoResp},
    service::stake::StackService,
};
use wallet_chain_interact::tron::operations::stake::ResourceType;
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
        req: FreezeBalanceReq,
        password: String,
    ) -> Result<FreezeResp, ServiceError> {
        WalletApplication::validate_password(self.ctx, &password).await?;

        let owner_ctx =
            ApiResourceDomain::withdraw_wallet_account_context(self.ctx, &req.owner_address)
                .await?;
        let resource = ResourceType::try_from(req.resource.as_str())?;
        let resource_trade_no = Self::client_resource_trade_no();
        self.create_client_resource_operation_pending(
            &owner_ctx.uid,
            &resource_trade_no,
            owner_ctx.owner_address,
            resource,
            req.frozen_balance.to_string(),
            ApiResourceOperationType::Stake,
        )
        .await?;

        let outcome = ApiResourceDomain::stake_withdraw_wallet_resource(self.ctx, &req).await?;

        self.complete_client_resource_operation_broadcast(
            &resource_trade_no,
            None,
            &outcome.tx_hash,
            &outcome.raw_tx,
            &outcome.transaction_fee,
        )
        .await?;

        outcome.resp.ok_or_else(|| {
            ServiceError::System(crate::error::system::SystemError::Internal(
                "api wallet stake response missing".to_string(),
            ))
        })
    }

    pub(crate) async fn unstake_withdraw_wallet_resource(
        &self,
        req: UnFreezeBalanceReq,
        password: String,
    ) -> Result<FreezeResp, ServiceError> {
        WalletApplication::validate_password(self.ctx, &password).await?;

        let owner_ctx =
            ApiResourceDomain::withdraw_wallet_account_context(self.ctx, &req.owner_address)
                .await?;
        let resource = ResourceType::try_from(req.resource.as_str())?;
        let resource_trade_no = Self::client_resource_trade_no();
        self.create_client_resource_operation_pending(
            &owner_ctx.uid,
            &resource_trade_no,
            owner_ctx.owner_address,
            resource,
            req.unfreeze_balance.to_string(),
            ApiResourceOperationType::Unstake,
        )
        .await?;

        let outcome = ApiResourceDomain::unstake_withdraw_wallet_resource(self.ctx, &req).await?;

        self.complete_client_resource_operation_broadcast(
            &resource_trade_no,
            None,
            &outcome.tx_hash,
            &outcome.raw_tx,
            &outcome.transaction_fee,
        )
        .await?;

        outcome.resp.ok_or_else(|| {
            ServiceError::System(crate::error::system::SystemError::Internal(
                "api wallet unstake response missing".to_string(),
            ))
        })
    }

    pub(crate) async fn withdraw_wallet_votes(
        &self,
        req: VoteWitnessReq,
        password: &str,
    ) -> Result<String, ServiceError> {
        WalletApplication::validate_password(self.ctx, password).await?;
        let owner_ctx =
            ApiResourceDomain::withdraw_wallet_account_context(self.ctx, &req.owner_address)
                .await?;
        let resource_trade_no = Self::client_resource_trade_no();
        self.create_client_resource_operation_pending(
            &owner_ctx.uid,
            &resource_trade_no,
            owner_ctx.owner_address,
            ResourceType::BANDWIDTH,
            req.get_votes().to_string(),
            ApiResourceOperationType::Vote,
        )
        .await?;

        let outcome = ApiResourceDomain::withdraw_wallet_votes(self.ctx, req).await?;
        self.complete_client_resource_operation_broadcast(
            &resource_trade_no,
            None,
            &outcome.tx_hash,
            &outcome.raw_tx,
            &outcome.transaction_fee,
        )
        .await?;
        Ok(outcome.tx_hash)
    }

    pub(crate) async fn withdraw_wallet_voter_info(
        &self,
        owner_address: &str,
    ) -> Result<VoterInfoResp, ServiceError> {
        ApiResourceDomain::withdraw_wallet_account_context(self.ctx, owner_address).await?;
        StackService::new(self.ctx).await?.voter_info(&owner_address).await
    }

    pub(crate) async fn withdraw_wallet_votes_node_list(
        &self,
        owner_address: Option<&str>,
    ) -> Result<VoteListResp, ServiceError> {
        if let Some(owner_address) = owner_address {
            ApiResourceDomain::withdraw_wallet_account_context(self.ctx, owner_address).await?;
        }
        StackService::new(self.ctx).await?.vote_list(owner_address).await
    }

    pub(crate) async fn withdraw_wallet_claim_votes_rewards(
        &self,
        req: WithdrawBalanceReq,
        password: &str,
    ) -> Result<String, ServiceError> {
        WalletApplication::validate_password(self.ctx, password).await?;
        let owner_ctx =
            ApiResourceDomain::withdraw_wallet_account_context(self.ctx, &req.owner_address)
                .await?;
        let resource_trade_no = Self::client_resource_trade_no();
        self.create_client_resource_operation_pending(
            &owner_ctx.uid,
            &resource_trade_no,
            owner_ctx.owner_address,
            ResourceType::BANDWIDTH,
            "0".to_string(),
            ApiResourceOperationType::WithdrawReward,
        )
        .await?;

        let outcome = ApiResourceDomain::withdraw_wallet_claim_votes_rewards(self.ctx, req).await?;
        self.complete_client_resource_operation_broadcast(
            &resource_trade_no,
            outcome.amount.clone(),
            &outcome.tx_hash,
            &outcome.raw_tx,
            &outcome.transaction_fee,
        )
        .await?;
        Ok(outcome.tx_hash)
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_client_resource_operation_pending(
        &self,
        uid: &str,
        resource_trade_no: &str,
        owner_address: String,
        resource: ResourceType,
        amount: String,
        operation_type: ApiResourceOperationType,
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
        ApiResourceOperationRepo::upsert(&pool, input)
            .await
            .map_err(|e| ServiceError::Database(e.into()))
    }

    async fn complete_client_resource_operation_broadcast(
        &self,
        resource_trade_no: &str,
        amount: Option<String>,
        tx_hash: &str,
        raw_tx: &str,
        transaction_fee: &str,
    ) -> Result<(), ServiceError> {
        let pool = self.ctx.api_transaction_pool()?;
        let existing = ApiResourceOperationRepo::get_by_resource_trade_no(&pool, resource_trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        let input = NewApiResourceOperation::client(
            existing.uid,
            existing.resource_trade_no,
            existing.owner_address,
            existing.resource_type,
            amount.unwrap_or(existing.amount),
            existing.operation_type,
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

    fn db_resource_type(resource: ResourceType) -> DbApiResourceType {
        match resource {
            ResourceType::BANDWIDTH => DbApiResourceType::Bandwidth,
            ResourceType::ENERGY => DbApiResourceType::Energy,
        }
    }

    fn client_resource_trade_no() -> String {
        format!("client_rsc_{}", uuid::Uuid::new_v4().simple())
    }
}
