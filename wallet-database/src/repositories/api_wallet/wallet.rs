use crate::{
    CoreDbPool,
    dao::api_wallet::ApiWalletDao,
    entities::api_wallet::{ApiWalletEntity, ApiWalletType},
};

pub struct ApiWalletRepo;

impl ApiWalletRepo {
    pub async fn upsert(
        pool: &CoreDbPool,
        uid: &str,
        name: &str,
        address: &str,
        phrase: &str,
        seed: &str,
        wallet_type: ApiWalletType,
        binding_address: Option<&str>,
        sn: &str,
    ) -> Result<ApiWalletEntity, crate::Error> {
        Ok(ApiWalletDao::upsert(
            pool.as_ref(),
            address,
            uid,
            name,
            phrase,
            seed,
            1,
            wallet_type,
            binding_address,
            sn,
        )
        .await?)
    }

    pub async fn edit_name(pool: &CoreDbPool, address: &str, name: &str) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::edit_name(pool.as_ref(), address, name).await?)
    }

    pub async fn update_merchant_id(
        pool: &CoreDbPool,
        address: &str,
        merchant_id: &str,
    ) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::update_merchain_id(pool.as_ref(), address, merchant_id).await?)
    }

    pub async fn update_app_id(
        pool: &CoreDbPool,
        address: &str,
        app_id: Option<&str>,
    ) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::update_app_id(pool.as_ref(), address, app_id).await?)
    }

    pub async fn update_sn(pool: &CoreDbPool, address: &str, sn: &str) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::update_sn(pool.as_ref(), address, sn).await?)
    }

    pub async fn update_seed_and_phrase(
        pool: &CoreDbPool,
        uid: &str,
        phrase: &str,
        seed: &str,
    ) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::update_seed_and_phrase(pool.as_ref(), uid, phrase, seed).await?)
    }

    pub async fn unbind_uid(pool: &CoreDbPool, address: &str) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::unbind_uid(pool.as_ref(), address).await?)
    }

    pub async fn mark_init(pool: &CoreDbPool, uid: &str) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::mark_init(pool.as_ref(), uid).await?)
    }

    pub async fn physical_delete(
        pool: &CoreDbPool,
        wallet_addresses: &[&str],
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::physical_delete(pool.as_ref(), wallet_addresses).await?)
    }

    pub async fn physical_delete_all_wallet(pool: &CoreDbPool) -> Result<u64, crate::Error> {
        Ok(ApiWalletDao::physical_delete_all_wallet(pool.as_ref()).await?)
    }

    pub async fn list(
        pool: &CoreDbPool,
        api_wallet_type: Option<ApiWalletType>,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::list(pool.as_ref(), api_wallet_type).await?)
    }

    pub async fn find_by_address(
        pool: &CoreDbPool,
        address: &str,
    ) -> Result<Option<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::detail(pool.as_ref(), address).await?)
    }
    pub async fn find_by_uid(
        pool: &CoreDbPool,
        uid: &str,
    ) -> Result<Option<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::detail_by_uid(pool.as_ref(), uid).await?)
    }

    pub async fn bind_withdraw_and_subaccount_relation(
        pool: &CoreDbPool,
        wallet_address: &str,
        binding_address: &str,
    ) -> Result<(), crate::Error> {
        ApiWalletDao::bind_withdraw_and_subaccount_relation(
            pool.as_ref(),
            binding_address,
            wallet_address,
        )
        .await?;
        ApiWalletDao::bind_withdraw_and_subaccount_relation(
            pool.as_ref(),
            wallet_address,
            binding_address,
        )
        .await
    }

    pub async fn wallet_latest(pool: &CoreDbPool) -> Result<Option<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::wallet_latest(pool.as_ref()).await?)
    }

    // TODO: 想办法用CoreDbPool替换executor
    pub async fn uid_list<'a, E>(executor: E) -> Result<Vec<(String,)>, crate::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Sqlite>,
    {
        Ok(ApiWalletDao::uid_list(executor).await?)
    }
}
