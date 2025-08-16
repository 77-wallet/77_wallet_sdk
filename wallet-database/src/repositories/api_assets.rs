use crate::dao::{api_assets::ApiAssetsDao, assets::CreateAssetsVo};

pub struct ApiAssetsRepo;

impl ApiAssetsRepo {
    pub async fn upsert(db_pool: crate::DbPool, input: CreateAssetsVo) -> Result<(), crate::Error> {
        Ok(ApiAssetsDao::upsert_assets(db_pool.as_ref(), input).await?)
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
