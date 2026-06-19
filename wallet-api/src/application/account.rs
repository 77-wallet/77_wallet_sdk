use crate::{
    api::ReturnType,
    context::Context,
    domain::{api_wallet::account::ApiAccountDomain, wallet::WalletDomain},
    error::service::ServiceError,
    response_vo::api_wallet::account::ApiWalletAddressSearchResp,
};
use wallet_chain_interact::types::ChainPrivateKey;

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

    pub async fn get_account_private_key(
        &self,
        address: &str,
        chain_code: &str,
        password: &str,
    ) -> Result<ChainPrivateKey, ServiceError> {
        WalletDomain::validate_password_with_context(self.ctx, password).await?;
        Ok(ApiAccountDomain::get_private_key_with_ctx(self.ctx, address, chain_code).await?)
    }
}
