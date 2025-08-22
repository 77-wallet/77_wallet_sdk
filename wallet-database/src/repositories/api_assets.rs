use crate::{
    DbPool,
    dao::api_assets::ApiAssetsDao,
    entities::{
        api_assets::{ApiAssetsEntity, ApiCreateAssetsVo},
        assets::AssetsId,
    },
};

pub struct ApiAssetsRepo;

impl ApiAssetsRepo {
    pub async fn upsert_assets(
        pool: &DbPool,
        assets: ApiCreateAssetsVo,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::upsert_assets(pool.as_ref(), assets).await
    }

    pub async fn update_balance(
        pool: &DbPool,
        id: &AssetsId,
        balance: &str,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::update_balance(pool.as_ref(), id, balance).await
    }

    pub async fn update_status(
        pool: &DbPool,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        status: u8,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::update_status(pool.as_ref(), chain_code, symbol, token_address, status).await
    }

    pub async fn find_by_id(
        pool: &DbPool,
        id: &AssetsId,
    ) -> Result<Option<ApiAssetsEntity>, crate::Error> {
        Ok(ApiAssetsDao::assets_by_id(pool.as_ref(), id).await?)
    }

    // pub async fn mark_as_unused(
    //     pool: &DbPool,
    //     wallet_address: &str,
    //     account_id: u32,
    // ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
    //     Ok(
    //         ApiAccountEntity::update_is_used(pool.as_ref(), wallet_address, account_id, false)
    //             .await?,
    //     )
    // }

    // pub async fn update(
    //     &mut self,
    //     id: u32,
    //     name: &str,
    //     address: &str,
    //     chain_code: &str,
    // ) -> Result<Option<ApiAccountEntity>, crate::Error> {
    //     let pool = self.repo.pool().clone();
    //     Ok(ApiAccountEntity::update(pool.as_ref(), id, name, address, chain_code).await?)
    // }

    // pub async fn find_by_conditions(
    //     &mut self,
    //     conditions: Vec<(&str, &str)>,
    // ) -> Result<Option<ApiAccountEntity>, crate::Error> {
    //     Ok(ApiAccountEntity::find_condition(self.repo.pool().as_ref(), conditions).await?)
    // }

    // pub async fn check_not_self(
    //     &mut self,
    //     id: u32,
    //     address: &str,
    //     chain_code: &str,
    // ) -> Result<Option<ApiAccountEntity>, crate::Error> {
    //     Ok(
    //         ApiAccountEntity::check_not_self(self.repo.pool().as_ref(), id, address, chain_code)
    //             .await?,
    //     )
    // }

    // pub async fn list(&mut self) -> Result<Vec<ApiAccountEntity>, crate::Error> {
    //     Ok(ApiAccountEntity::list(self.repo.pool().as_ref()).await?)
    // }

    // pub async fn find_by_address(
    //     &mut self,
    //     address: &str,
    // ) -> Result<Option<ApiAccountEntity>, crate::Error> {
    //     Ok(ApiAccountEntity::detail(self.repo.pool().as_ref(), address).await?)
    // }
}
