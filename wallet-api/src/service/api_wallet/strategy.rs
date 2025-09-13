use wallet_transport_backend::{
    request::api_wallet::strategy::{ChainConfig, SaveCollectStrategyReq, SaveWithdrawStrategyReq},
    response_vo::api_wallet::strategy::{CollectionStrategyResp, WithdrawStrategyResp},
};

pub struct StrategyService {}

impl StrategyService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn get_collection_strategy(
        self,
        uid: &str,
    ) -> Result<CollectionStrategyResp, crate::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let resp = backend_api.query_collect_strategy(uid).await?;
        Ok(resp)
    }

    pub async fn update_collection_strategy(
        self,
        uid: &str,
        threshold: f64,
        chain_config: Vec<ChainConfig>,
    ) -> Result<(), crate::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let req = SaveCollectStrategyReq::new(uid, threshold, chain_config);
        backend_api.save_collect_strategy(&req).await?;

        Ok(())
    }

    pub async fn get_withdraw_strategy(
        self,
        uid: &str,
    ) -> Result<WithdrawStrategyResp, crate::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let resp = backend_api.query_withdrawal_strategy(uid).await?;
        Ok(resp)
    }

    pub async fn update_withdraw_strategy(
        self,
        uid: &str,
        threshold: f64,
        chain_config: Vec<ChainConfig>,
    ) -> Result<(), crate::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let req = SaveWithdrawStrategyReq::new(uid, threshold, chain_config);
        backend_api.save_withdrawal_strategy(&req).await?;

        Ok(())
    }
}
