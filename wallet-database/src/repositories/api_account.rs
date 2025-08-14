use crate::{
    entities::{
        api_account::{ApiAccountEntity, CreateApiAccountVo},
        api_wallet::ApiWalletType,
    },
    DbPool,
};

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
    pub async fn upsert(pool: &DbPool, input: Vec<CreateApiAccountVo>) -> Result<(), crate::Error> {
        Ok(ApiAccountEntity::upsert_multi(pool.as_ref(), input).await?)
    }

    pub async fn mark_as_used(
        pool: &DbPool,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        Ok(
            ApiAccountEntity::update_is_used(pool.as_ref(), wallet_address, account_id, true)
                .await?,
        )
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
    pub async fn find_one(
        pool: &DbPool,
        address: &str,
        chain_code: &str,
        address_type: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Option<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountEntity::find_one(
            pool.as_ref(),
            address,
            chain_code,
            address_type,
            api_wallet_type,
        )
        .await?)
    }

    pub async fn find_one_by_wallet_address_index(
        pool: &DbPool,
        wallet_address: &str,
        chain_code: &str,
        account_id: u32,
    ) -> Result<Option<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountEntity::find_one_by_wallet_address_index(
            pool.as_ref(),
            wallet_address,
            chain_code,
            account_id,
        )
        .await?)
    }

    pub async fn has_account_id(
        pool: &DbPool,
        wallet_address: &str,
        account_id: u32,
        api_wallet_type: ApiWalletType,
    ) -> Result<bool, crate::Error> {
        Ok(ApiAccountEntity::has_account_id(
            pool.as_ref(),
            wallet_address,
            account_id,
            api_wallet_type,
        )
        .await?)
    }

    pub async fn account_detail_by_max_id_and_wallet_address(
        pool: &DbPool,
        wallet_address: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Option<ApiAccountEntity>, crate::Error> {
        Ok(
            ApiAccountEntity::account_detail_by_max_id_and_wallet_address(
                pool.as_ref(),
                wallet_address,
                api_wallet_type,
            )
            .await?,
        )
    }
}
