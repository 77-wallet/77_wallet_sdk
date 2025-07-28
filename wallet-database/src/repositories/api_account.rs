use crate::entities::api_account::ApiAccountEntity;

use super::ResourcesRepo;

pub struct ApiAccountRepo {
    repo: ResourcesRepo,
}

impl ApiAccountRepo {
    pub fn new(db_pool: crate::DbPool) -> Self {
        Self {
            repo: ResourcesRepo::new(db_pool),
        }
    }
}

impl ApiAccountRepo {
    // pub async fn insert(
    //     &mut self,
    //     name: &str,
    //     address: &str,
    //     chain_code: &str,
    // ) -> Result<Option<ApiAccountEntity>, crate::Error> {
    //     Ok(ApiAccountEntity::upsert(self.repo.pool().as_ref(), name, address, chain_code).await?)
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

    pub async fn delete(
        &mut self,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        Ok(
            ApiAccountEntity::physical_delete(
                self.repo.pool().as_ref(),
                wallet_address,
                account_id,
            )
            .await?,
        )
    }

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
