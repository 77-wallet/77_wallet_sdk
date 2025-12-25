use crate::{context::Context, domain::api_wallet::strategy::StrategyDomain};
use wallet_transport_backend::{
    request::api_wallet::strategy::{ChainConfig, SaveCollectStrategyReq, SaveWithdrawStrategyReq},
    response_vo::api_wallet::strategy::{CollectionStrategyResp, WithdrawStrategyResp},
};

pub struct StrategyService {
    ctx: &'static Context,
}

impl StrategyService {
    pub fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub async fn update_collect_strategy(
        self,
        uid: &str,
        threshold: u32,
        chain_config: Vec<ChainConfig>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let backend_api = self.ctx.get_global_backend_api();
        let req = SaveCollectStrategyReq::new(uid, threshold, chain_config);
        let strategy_domain = StrategyDomain {};

        // 1. 调用后端API保存策略
        backend_api.save_collect_strategy(&req).await?;

        // 2. 保存到本地数据库
        strategy_domain.save_local_collect_strategy(uid, &req).await?;

        Ok(())
    }

    pub async fn query_collect_strategy(
        self,
        uid: &str,
    ) -> Result<CollectionStrategyResp, crate::error::service::ServiceError> {
        // 使用本地优先的策略查询逻辑
        let strategy_domain = StrategyDomain {};
        let strategy = strategy_domain.query_collect_strategy(uid).await?;

        Ok(strategy)
    }

    pub async fn update_withdrawal_strategy(
        self,
        uid: &str,
        threshold: u32,
        chain_config: Vec<ChainConfig>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let backend_api = self.ctx.get_global_backend_api();
        let req = SaveWithdrawStrategyReq::new(uid, threshold, chain_config);

        // 1. 调用后端API保存策略
        backend_api.save_withdrawal_strategy(&req).await?;

        // 2. 保存到本地数据库
        StrategyDomain::save_local_withdraw_strategy(uid, &req).await?;

        Ok(())
    }

    pub async fn query_withdrawal_strategy(
        self,
        uid: &str,
    ) -> Result<WithdrawStrategyResp, crate::error::service::ServiceError> {
        // 使用本地优先的策略查询逻辑
        let strategy = StrategyDomain::query_withdraw_strategy(uid).await?;

        Ok(strategy)
    }

    pub async fn query_api_wallet_configs(
        self,
    ) -> Result<serde_json::Value, crate::error::service::ServiceError> {
        let backend_api = self.ctx.get_global_backend_api();
        let res = backend_api.query_api_wallet_configs().await?;
        Ok(res)
    }
}
