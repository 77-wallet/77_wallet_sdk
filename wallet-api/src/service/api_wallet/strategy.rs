use crate::context::Context;
use wallet_transport_backend::{
    request::api_wallet::strategy::{ChainConfig, SaveCollectStrategyReq, SaveWithdrawStrategyReq},
    response_vo::api_wallet::strategy::WithdrawStrategyResp,
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
        threshold: f64,
        chain_config: Vec<ChainConfig>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let backend_api = self.ctx.get_global_backend_api();
        let req = SaveCollectStrategyReq::new(uid, threshold, chain_config);
        backend_api.save_collect_strategy(&req).await?;

        Ok(())
    }

    pub async fn query_collect_strategy(
        self,
        uid: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let backend_api = self.ctx.get_global_backend_api();
        backend_api.query_collect_strategy(&uid).await?;

        Ok(())
    }

    pub async fn update_withdrawal_strategy(
        self,
        uid: &str,
        threshold: f64,
        chain_config: Vec<ChainConfig>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let backend_api = self.ctx.get_global_backend_api();

        let req = SaveWithdrawStrategyReq::new(uid, threshold, chain_config);
        backend_api.save_withdrawal_strategy(&req).await?;

        Ok(())
    }

    pub async fn query_withdrawal_strategy(
        self,
        uid: &str,
    ) -> Result<WithdrawStrategyResp, crate::error::service::ServiceError> {
        let backend_api = self.ctx.get_global_backend_api();
        let res = backend_api.query_withdrawal_strategy(&uid).await?;
        Ok(res)
    }
}
