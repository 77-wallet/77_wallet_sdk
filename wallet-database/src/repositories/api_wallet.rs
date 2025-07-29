use crate::{
    entities::api_wallet::{ApiWalletEntity, ApiWalletType},
    DbPool,
};

use super::ResourcesRepo;

pub struct ApiWalletRepo {
    repo: ResourcesRepo,
}

impl ApiWalletRepo {
    pub fn new(db_pool: crate::DbPool) -> Self {
        Self {
            repo: ResourcesRepo::new(db_pool),
        }
    }
}

impl ApiWalletRepo {
    pub async fn upsert(
        pool: &DbPool,
        uid: &str,
        name: &str,
        address: &str,
        phrase: &str,
        seed: &str,
        wallet_type: ApiWalletType,
    ) -> Result<ApiWalletEntity, crate::Error> {
        Ok(ApiWalletEntity::upsert(
            pool.as_ref(),
            address,
            uid,
            name,
            phrase,
            seed,
            1,
            wallet_type,
        )
        .await?)
    }

    pub async fn update_merchant_id(
        pool: &DbPool,
        address: &str,
        merchant_id: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletEntity::update_merchain_id(
            pool.as_ref(),
            address,
            merchant_id,
            api_wallet_type,
        )
        .await?)
    }

    pub async fn update_app_id(
        pool: &DbPool,
        address: &str,
        app_id: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletEntity::update_app_id(pool.as_ref(), address, app_id, api_wallet_type).await?)
    }

    // pub async fn update(
    //     &mut self,
    //     id: u32,
    //     name: &str,
    //     address: &str,
    //     chain_code: &str,
    // ) -> Result<Option<ApiWalletEntity>, crate::Error> {
    //     let pool = self.repo.pool().clone();
    //     Ok(ApiWalletEntity::update(pool.as_ref(), id, name, address, chain_code).await?)
    // }

    // pub async fn find_by_conditions(
    //     &mut self,
    //     conditions: Vec<(&str, &str)>,
    // ) -> Result<Option<ApiWalletEntity>, crate::Error> {
    //     Ok(ApiWalletEntity::find_condition(self.repo.pool().as_ref(), conditions).await?)
    // }

    // pub async fn check_not_self(
    //     &mut self,
    //     id: u32,
    //     address: &str,
    //     chain_code: &str,
    // ) -> Result<Option<ApiWalletEntity>, crate::Error> {
    //     Ok(
    //         ApiWalletEntity::check_not_self(self.repo.pool().as_ref(), id, address, chain_code)
    //             .await?,
    //     )
    // }

    pub async fn delete(
        &mut self,
        wallet_addresses: &[&str],
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletEntity::delete_by_address(self.repo.pool().as_ref(), wallet_addresses).await?)
    }

    pub async fn list(
        pool: &DbPool,
        address: Option<&str>,
        api_wallet_type: Option<ApiWalletType>,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletEntity::list(pool.as_ref(), address, api_wallet_type).await?)
    }

    pub async fn find_by_address(
        pool: &DbPool,
        address: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Option<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletEntity::detail(pool.as_ref(), address, api_wallet_type).await?)
    }
    pub async fn find_by_uid(
        pool: &DbPool,
        uid: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Option<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletEntity::detail_by_uid(pool.as_ref(), uid, api_wallet_type).await?)
    }
}
