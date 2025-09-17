use wallet_transport_backend::request::api_wallet::strategy::{
    ChainConfig, SaveCollectStrategyReq, SaveWithdrawStrategyReq,
};

pub struct StrategyService {}

impl StrategyService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn update_collect_strategy(
        self,
        uid: &str,
        threshold: f64,
        chain_config: Vec<ChainConfig>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let req = SaveCollectStrategyReq::new(uid, threshold, chain_config);
        backend_api.save_collect_strategy(&req).await?;

        Ok(())
    }

    pub async fn query_collect_strategy(
        self,
        uid: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        backend_api.query_collect_strategy(&uid).await?;

        Ok(())
    }

    pub async fn update_withdrawal_strategy(
        self,
        uid: &str,
        threshold: f64,
        chain_config: Vec<ChainConfig>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let req = SaveWithdrawStrategyReq::new(uid, threshold, chain_config);
        backend_api.save_withdrawal_strategy(&req).await?;

        Ok(())
    }

    pub async fn query_withdrawal_strategy(
        self,
        uid: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        backend_api.query_withdrawal_strategy(&uid).await?;

        Ok(())
    }
}
