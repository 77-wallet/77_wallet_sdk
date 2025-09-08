use crate::{
    api::ReturnType,
    infrastructure::swap_client::DefaultQuoteResp,
    request::transaction::{ApproveReq, QuoteReq, SwapReq, SwapTokenListReq},
    response_vo::{
        swap::{ApiQuoteResp, ApproveList, SwapTokenInfo},
        EstimateFeeResp,
    },
    service::swap::SwapServer,
};
use wallet_database::pagination::Pagination;
use wallet_transport_backend::api::swap::ChainDex;

impl crate::WalletManager {
    pub async fn default_quote(
        &self,
        chain_code: String,
        token_in: String,
    ) -> ReturnType<DefaultQuoteResp> {
        SwapServer::new()?
            .default_quote(chain_code, token_in)
            .await
            .into()
    }

    // 获取报价
    pub async fn quote(&self, req: QuoteReq) -> ReturnType<ApiQuoteResp> {
        SwapServer::new()?.quote(req).await.into()
    }

    pub async fn swap(&self, req: SwapReq, fee: String, password: String) -> ReturnType<String> {
        SwapServer::new()?.swap(req, fee, password).await.into()
    }

    // 获取token列表
    pub async fn token_list(&self, req: SwapTokenListReq) -> ReturnType<Pagination<SwapTokenInfo>> {
        SwapServer::new()?.token_list(req).await.into()
    }

    // 支持兑换的链
    pub async fn chain_list(&self) -> ReturnType<Vec<ChainDex>> {
        SwapServer::new()?.chain_list().await.into()
    }

    pub async fn approve(&self, req: ApproveReq, password: String) -> ReturnType<String> {
        SwapServer::new()?.approve(req, password).await.into()
    }

    pub async fn approve_fee(
        &self,
        req: ApproveReq,
        is_cancel: bool,
    ) -> ReturnType<EstimateFeeResp> {
        SwapServer::new()?.approve_fee(req, is_cancel).await.into()
    }

    pub async fn approve_list(&self, uid: String, account_id: u32) -> ReturnType<Vec<ApproveList>> {
        SwapServer::new()?
            .approve_list(uid, account_id)
            .await
            .into()
    }

    pub async fn approve_cancel(&self, req: ApproveReq, password: String) -> ReturnType<String> {
        SwapServer::new()?
            .approve_cancel(req, password)
            .await
            .into()
    }
}

#[cfg(test)]
mod tests {
    use crate::test::env::get_manager;
    use anyhow::Result;

    #[tokio::test]
    async fn test_default_quote() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let chain_code = "doge".to_string();
        let token_in = "".to_string();
        // let token_out = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string();

        let resp = wallet_manager.default_quote(chain_code, token_in).await;
        println!("{}", serde_json::to_string(&resp).unwrap());
        Ok(())
    }
}
