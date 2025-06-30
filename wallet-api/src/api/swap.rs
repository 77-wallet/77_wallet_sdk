use crate::{
    api::ReturnType,
    request::transaction::{ApproveParams, DepositParams, QuoteReq, SwapReq},
    response_vo::swap::{ApiQuoteResp, SupportChain, SwapTokenInfo},
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
    pub async fn token_list(&self) -> ReturnType<Vec<SwapTokenInfo>> {
        SwapServer::new()?.token_list().await.into()
    }

    // 支持兑换的链
    pub async fn chain_list(&self) -> ReturnType<Vec<SupportChain>> {
        SwapServer::new()?.chain_list().await.into()
    }

    pub async fn approve(
        &self,
        req: ApproveParams,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        SwapServer::new()?.approve(req, password).await.into()
    }

    pub async fn deposit(
        &self,
        req: DepositParams,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        SwapServer::new()?.deposit(req, password).await.into()
    }
}
