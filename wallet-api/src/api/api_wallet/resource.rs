use crate::{
    api::ReturnType,
    manager::WalletManager,
    request::api_wallet::resource::{ApiResourceStakeReq, ApiResourceUnstakeReq},
    response_vo::api_wallet::resource::ApiResourceOperationResp,
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
}
