use crate::{
    api::ReturnType,
    infrastructure::swap_client::DefaultQuoteResp,
    manager::WalletManager,
    request::transaction::{ApproveReq, QuoteReq, SwapReq, SwapTokenListReq},
    response_vo::{
        EstimateFeeResp,
        standard_wallet::swap::{ApiQuoteResp, ApproveList, SwapTokenInfo},
    },
    service::swap::SwapServer,
};
use wallet_database::pagination::Pagination;
use wallet_transport_backend::api::wallet::swap::ChainDex;

impl WalletManager {
    fn swap_server(&self) -> ReturnType<SwapServer> {
        SwapServer::new(self.ctx)
    }

    pub async fn default_quote(
        &self,
        chain_code: String,
        token_in: String,
    ) -> ReturnType<DefaultQuoteResp> {
        self.swap_server()?.default_quote(chain_code, token_in).await
    }

    // 获取报价
    pub async fn quote(&self, req: QuoteReq) -> ReturnType<ApiQuoteResp> {
        self.swap_server()?.quote(req).await
    }

    pub async fn swap(&self, req: SwapReq, fee: String, password: String) -> ReturnType<String> {
        self.swap_server()?.swap(req, fee, password).await
    }

    // 获取token列表
    pub async fn token_list(&self, req: SwapTokenListReq) -> ReturnType<Pagination<SwapTokenInfo>> {
        self.swap_server()?.token_list(req).await
    }

    // 支持兑换的链
    pub async fn chain_list(&self) -> ReturnType<Vec<ChainDex>> {
        self.swap_server()?.chain_list().await
    }

    pub async fn approve(&self, req: ApproveReq, password: String) -> ReturnType<String> {
        self.swap_server()?.approve(req, password).await
    }

    pub async fn approve_fee(
        &self,
        req: ApproveReq,
        is_cancel: bool,
    ) -> ReturnType<EstimateFeeResp> {
        self.swap_server()?.approve_fee(req, is_cancel).await
    }

    pub async fn approve_list(&self, uid: String, account_id: u32) -> ReturnType<Vec<ApproveList>> {
        self.swap_server()?.approve_list(uid, account_id).await
    }

    pub async fn approve_cancel(&self, req: ApproveReq, password: String) -> ReturnType<String> {
        self.swap_server()?.approve_cancel(req, password).await
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod tests {
    use crate::testkit::env::get_manager;
    use anyhow::Result;

    #[tokio::test]
    async fn test_default_quote() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let chain_code = "doge".to_string();
        let token_in = "".to_string();
        // let token_out = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string();

        let resp = wallet_manager.default_quote(chain_code, token_in).await?;
        println!("{}", serde_json::to_string(&resp).unwrap());
        Ok(())
    }
}
