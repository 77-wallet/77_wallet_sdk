use crate::{
    api::ReturnType,
    response_vo::swap::{SupportChain, SwapTokenInfo},
    service::swap::SwapServer,
};

impl crate::WalletManager {
    // 获取报价
    pub async fn quote(&self) -> ReturnType<()> {
        // 查询路径，模拟执行
        SwapServer::new().quote().await.into()
    }

    // 获取token列表
    pub async fn token_list(&self) -> ReturnType<Vec<SwapTokenInfo>> {
        SwapServer::new().token_list().await.into()
    }

    // 支持兑换的链
    pub async fn chain_list(&self) -> ReturnType<Vec<SupportChain>> {
        SwapServer::new().chain_list().await.into()
    }
}
