use crate::{
    api::ReturnType,
    domain::swap_client::{DefaultQuoteResp, SupportChain, SupportDex},
    request::transaction::{ApproveReq, QuoteReq, SwapReq, SwapTokenListReq},
    response_vo::{
        swap::{ApiQuoteResp, ApproveList, SwapTokenInfo},
        EstimateFeeResp,
    },
    service::swap::SwapServer,
};
use wallet_database::pagination::Pagination;

impl crate::WalletManager {
    pub async fn default_quote(
        &self,
        chain_code: String,
        token_in: String,
        token_out: String,
    ) -> ReturnType<DefaultQuoteResp> {
        SwapServer::new()?
            .default_quote(chain_code, token_in, token_out)
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
    pub async fn chain_list(&self) -> ReturnType<Vec<SupportChain>> {
        SwapServer::new()?.chain_list().await.into()
    }

    pub async fn dex_list(&self, chain_code: String) -> ReturnType<Vec<SupportDex>> {
        SwapServer::new()?.dex_list(chain_code).await.into()
    }

    pub async fn approve(&self, req: ApproveReq, password: String) -> ReturnType<String> {
        SwapServer::new()?.approve(req, password).await.into()
    }

    pub async fn approve_fee(&self, req: ApproveReq) -> ReturnType<EstimateFeeResp> {
        SwapServer::new()?.approve_fee(req).await.into()
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

    pub async fn supplier(&self, chain_code: String) -> ReturnType<serde_json::Value> {
        SwapServer::new()?.supplier(chain_code).await.into()
    }
}
