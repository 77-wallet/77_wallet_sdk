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
        WalletApplication::validate_password(&password).await?;

        let outcome = ApiResourceDomain::stake_withdraw_wallet_resource(self.ctx, &req).await?;

        let resource_trade_no = Self::client_resource_trade_no();
        self.record_client_resource_operation(
            outcome.uid.as_deref().ok_or_else(|| {
                ServiceError::System(crate::error::system::SystemError::Internal(
                    "api wallet stake uid missing".to_string(),
                ))
            })?,
            &resource_trade_no,
            outcome.owner_address.clone(),
            outcome.resource_type.ok_or_else(|| {
                ServiceError::System(crate::error::system::SystemError::Internal(
                    "api wallet stake resource type missing".to_string(),
                ))
            })?,
            req.frozen_balance,
            ApiResourceOperationType::Stake,
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
        WalletApplication::validate_password(&password).await?;

        let outcome = ApiResourceDomain::unstake_withdraw_wallet_resource(self.ctx, &req).await?;

        let resource_trade_no = Self::client_resource_trade_no();
        self.record_client_resource_operation(
            outcome.uid.as_deref().ok_or_else(|| {
                ServiceError::System(crate::error::system::SystemError::Internal(
                    "api wallet unstake uid missing".to_string(),
                ))
            })?,
            &resource_trade_no,
            outcome.owner_address.clone(),
            outcome.resource_type.ok_or_else(|| {
                ServiceError::System(crate::error::system::SystemError::Internal(
                    "api wallet unstake resource type missing".to_string(),
                ))
            })?,
            req.unfreeze_balance,
            ApiResourceOperationType::Unstake,
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
        WalletApplication::validate_password(password).await?;
        ApiResourceDomain::withdraw_wallet_votes(self.ctx, req).await
    }

    pub(crate) async fn withdraw_wallet_voter_info(
        &self,
        owner_address: &str,
    ) -> Result<VoterInfoResp, ServiceError> {
        ApiResourceDomain::withdraw_wallet_account_context(self.ctx, owner_address).await?;
        StackService::new().await?.voter_info(&owner_address).await
    }

    pub(crate) async fn withdraw_wallet_votes_node_list(
        &self,
        owner_address: Option<&str>,
    ) -> Result<VoteListResp, ServiceError> {
        if let Some(owner_address) = owner_address {
            ApiResourceDomain::withdraw_wallet_account_context(self.ctx, owner_address).await?;
        }
        StackService::new().await?.vote_list(owner_address).await
    }

    pub(crate) async fn withdraw_wallet_claim_votes_rewards(
        &self,
        req: WithdrawBalanceReq,
        password: &str,
    ) -> Result<String, ServiceError> {
        WalletApplication::validate_password(password).await?;
        ApiResourceDomain::withdraw_wallet_claim_votes_rewards(self.ctx, req).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_client_resource_operation(
        &self,
        uid: &str,
        resource_trade_no: &str,
        owner_address: String,
        resource: ResourceType,
        amount: i64,
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
            amount.to_string(),
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
