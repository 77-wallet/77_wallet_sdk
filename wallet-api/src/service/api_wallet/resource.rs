use crate::{
    application::api_wallet_resource::ApiResourceApplication,
    context::Context,
    error::service::ServiceError,
    request::api_wallet::resource::{ApiResourceStakeReq, ApiResourceUnstakeReq},
    response_vo::api_wallet::resource::ApiResourceOperationResp,
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
}
