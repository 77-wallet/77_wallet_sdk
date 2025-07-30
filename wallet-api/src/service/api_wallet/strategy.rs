use wallet_database::repositories::ResourcesRepo;
use wallet_transport_backend::request::api_wallet::strategy::{
    SaveOrUpdateCollectionStrategyReq, SaveOrUpdateWithdrawStrategyReq,
};

pub struct StrategyService {
    pub repo: ResourcesRepo,
}

impl StrategyService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo }
    }

    pub async fn update_collection_strategy(self) -> Result<(), crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;

        let req = SaveOrUpdateCollectionStrategyReq::new();
        backend_api.save_or_update_collection_strategy(&req).await?;

        Ok(())
    }

    pub async fn update_withdraw_strategy(self) -> Result<(), crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;

        let req = SaveOrUpdateWithdrawStrategyReq::new();
        backend_api.save_or_update_withdraw_strategy(&req).await?;

        Ok(())
    }
}
