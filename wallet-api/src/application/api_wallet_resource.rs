use crate::{
    application::wallet::WalletApplication,
    context::Context,
    domain::api_wallet::resource::ApiResourceDomain,
    error::service::ServiceError,
    request::api_wallet::resource::{ApiResourceStakeReq, ApiResourceUnstakeReq},
    response_vo::api_wallet::resource::ApiResourceOperationResp,
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

        Ok(ApiResourceOperationResp::success(uuid::Uuid::new_v4().to_string(), outcome.tx_hash))
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

        Ok(ApiResourceOperationResp::success(uuid::Uuid::new_v4().to_string(), outcome.tx_hash))
    }
}
