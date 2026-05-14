use crate::{
    api::ReturnType, context::Context, domain::api_wallet::account::ApiAccountDomain,
    request::api_wallet::account::ApiWalletAddressSearchReq,
    response_vo::api_wallet::account::ApiWalletAddressSearchResp,
    service::api_wallet::account::ApiAccountService,
};

pub(crate) struct ApiResourceApplication {
    ctx: &'static Context,
}

impl ApiResourceApplication {
    pub fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub async fn search_api_wallet_address(
        &self,
        req: ApiWalletAddressSearchReq,
    ) -> ReturnType<ApiWalletAddressSearchResp> {
        ApiAccountDomain::search_address(&req.wallet_address, &req.keyword).await
    }
}
