use crate::{
    api::ReturnType,
    domain::swap_client::{SupportChain, SupportDex},
    request::transaction::{ApproveReq, QuoteReq, SwapReq, SwapTokenListReq},
    response_vo::swap::ApiQuoteResp,
    service::swap::SwapServer,
};

impl crate::WalletManager {
    // 获取报价
    pub async fn quote(&self, req: QuoteReq) -> ReturnType<ApiQuoteResp> {
        SwapServer::new()?.quote(req).await.into()
    }

    pub async fn swap(&self, req: SwapReq, fee: String, password: String) -> ReturnType<String> {
        SwapServer::new()?.swap(req, fee, password).await.into()
    }

    // 获取token列表
    pub async fn token_list(&self, req: SwapTokenListReq) -> ReturnType<serde_json::Value> {
        SwapServer::new()?.token_list(req).await.into()
    }

    // 支持兑换的链
    pub async fn chain_list(&self) -> ReturnType<Vec<SupportChain>> {
        SwapServer::new()?.chain_list().await.into()
    }

    pub async fn dex_list(&self, chain_code: String) -> ReturnType<Vec<SupportDex>> {
        SwapServer::new()?.dex_list(chain_code).await.into()
    }

    pub async fn approve(&self, req: ApproveReq, password: String) -> ReturnType<String> {
        SwapServer::new()?.approve(req, password).await.into()
    }

    pub async fn allowance(
        &self,
        from: String,
        token: String,
        chain_code: String,
        spender: String,
    ) -> Result<String, crate::ServiceError> {
        SwapServer::new()?
            .allowance(from, token, chain_code, spender)
            .await
            .into()
    }
}
