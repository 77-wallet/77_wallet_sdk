use crate::{
    api::ReturnType, context::Context, domain::api_wallet::account::ApiAccountDomain,
    response_vo::api_wallet::account::ApiWalletAddressSearchResp,
    service::api_wallet::account::ApiAccountService,
};

pub(crate) struct ApiAccountApplication {
    ctx: &'static Context,
}

impl ApiAccountApplication {
    pub fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub async fn search_api_wallet_address(
        &self,
        uid: &str,
        keyword: &str,
    ) -> ReturnType<ApiWalletAddressSearchResp> {
        ApiAccountDomain::search_address(self.ctx, uid, keyword).await
    }
}
