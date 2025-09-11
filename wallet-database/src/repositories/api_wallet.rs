use crate::{
    DbPool,
    dao::api_wallet::ApiWalletDao,
    entities::api_wallet::{ApiWalletEntity, ApiWalletType},
};

pub struct ApiWalletRepo;

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
        Ok(ApiWalletDao::upsert(pool.as_ref(), address, uid, name, phrase, seed, 1, wallet_type)
            .await?)
    }

    pub async fn edit_name(
        pool: &DbPool,
        address: &str,
        name: &str,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::edit_name(pool.as_ref(), address, name).await?)
    }

    pub async fn update_merchant_id(
        pool: &DbPool,
        address: &str,
        merchant_id: &str,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::update_merchain_id(pool.as_ref(), address, merchant_id).await?)
    }

    pub async fn update_app_id(
        pool: &DbPool,
        address: &str,
        app_id: &str,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::update_app_id(pool.as_ref(), address, app_id).await?)
    }

    pub async fn upbind_uid(
        pool: &DbPool,
        address: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::unbind_uid(pool.as_ref(), address, api_wallet_type).await?)
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
        pool: &DbPool,
        wallet_addresses: &[&str],
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::delete_by_address(pool.as_ref(), wallet_addresses).await?)
    }

    pub async fn list(
        pool: &DbPool,
        address: Option<&str>,
        api_wallet_type: Option<ApiWalletType>,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::list(pool.as_ref(), address, api_wallet_type).await?)
    }

    pub async fn find_by_address(
        pool: &DbPool,
        address: &str,
    ) -> Result<Option<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::detail(pool.as_ref(), address).await?)
    }
    pub async fn find_by_uid(
        pool: &DbPool,
        uid: &str,
    ) -> Result<Option<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::detail_by_uid(pool.as_ref(), uid).await?)
    }

    pub async fn bind_withdraw_and_subaccount_relation(
        pool: DbPool,
        wallet_address: &str,
        binding_address: &str,
    ) -> Result<(), crate::Error> {
        ApiWalletDao::bind_withdraw_and_subaccount_relation(
            pool.as_ref(),
            wallet_address,
            binding_address,
        )
        .await
    }
}
