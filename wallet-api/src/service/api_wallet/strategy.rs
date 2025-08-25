use wallet_database::repositories::ResourcesRepo;
use wallet_transport_backend::request::api_wallet::strategy::{
    ChainConfig, SaveCollectStrategyReq, SaveWithdrawStrategyReq,
};

pub struct StrategyService {
    pub repo: ResourcesRepo,
}

impl StrategyService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo }
    }

    pub async fn update_collection_strategy(
        self,
        uid: &str,
        threshold: f64,
        chain_config: Vec<ChainConfig>,
    ) -> Result<(), crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;

        let req = SaveCollectStrategyReq::new(uid, threshold, chain_config);
        backend_api.save_collection_strategy(&req).await?;

        Ok(())
    }

    pub async fn update_withdraw_strategy(
        self,
        uid: &str,
        threshold: f64,
        chain_config: Vec<ChainConfig>,
    ) -> Result<(), crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;

        let req = SaveWithdrawStrategyReq::new(uid, threshold, chain_config);
        backend_api.save_withdraw_strategy(&req).await?;

        Ok(())
    }
}
