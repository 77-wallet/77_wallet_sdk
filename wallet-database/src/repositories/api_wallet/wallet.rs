use crate::{
    ApiWalletDbPool,
    dao::api_wallet::ApiWalletDao,
    entities::api_wallet::{ApiWalletEntity, ApiWalletType},
};

pub struct ApiWalletRepo;

impl ApiWalletRepo {
    pub async fn upsert(
        pool: &ApiWalletDbPool,
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
            pool.write_ref(),
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

    pub async fn edit_name(
        pool: &ApiWalletDbPool,
        address: &str,
        name: &str,
    ) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::edit_name(pool.write_ref(), address, name).await?)
    }

    pub async fn update_merchant_id(
        pool: &ApiWalletDbPool,
        address: &str,
        merchant_id: &str,
    ) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::update_merchain_id(pool.write_ref(), address, merchant_id).await?)
    }

    pub async fn update_app_id(
        pool: &ApiWalletDbPool,
        address: &str,
        app_id: Option<&str>,
    ) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::update_app_id(pool.write_ref(), address, app_id).await?)
    }

    pub async fn update_sn(
        pool: &ApiWalletDbPool,
        address: &str,
        sn: &str,
    ) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::update_sn(pool.write_ref(), address, sn).await?)
    }

    pub async fn update_seed_and_phrase(
        pool: &ApiWalletDbPool,
        uid: &str,
        phrase: &str,
        seed: &str,
    ) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::update_seed_and_phrase(pool.write_ref(), uid, phrase, seed).await?)
    }

    pub async fn unbind_uid(pool: &ApiWalletDbPool, address: &str) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::unbind_uid(pool.write_ref(), address).await?)
    }

    pub async fn mark_init(pool: &ApiWalletDbPool, uid: &str) -> Result<bool, crate::Error> {
        Ok(ApiWalletDao::mark_init(pool.write_ref(), uid).await?)
    }

    pub async fn physical_delete(
        pool: &ApiWalletDbPool,
        wallet_addresses: &[&str],
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::physical_delete(pool.write_ref(), wallet_addresses).await?)
    }

    pub async fn physical_delete_all_wallet(pool: &ApiWalletDbPool) -> Result<u64, crate::Error> {
        Ok(ApiWalletDao::physical_delete_all_wallet(pool.write_ref()).await?)
    }

    pub async fn list(
        pool: &ApiWalletDbPool,
        api_wallet_type: Option<ApiWalletType>,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::list(pool.read_ref(), api_wallet_type).await?)
    }

    pub async fn find_by_address(
        pool: &ApiWalletDbPool,
        address: &str,
    ) -> Result<Option<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::detail(pool.read_ref(), address).await?)
    }
    pub async fn find_by_uid(
        pool: &ApiWalletDbPool,
        uid: &str,
    ) -> Result<Option<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::detail_by_uid(pool.read_ref(), uid).await?)
    }

    pub async fn bind_withdraw_and_subaccount_relation(
        pool: &ApiWalletDbPool,
        wallet_address: &str,
        binding_address: &str,
    ) -> Result<(), crate::Error> {
        ApiWalletDao::bind_withdraw_and_subaccount_relation(
            pool.write_ref(),
            binding_address,
            wallet_address,
        )
        .await?;
        ApiWalletDao::bind_withdraw_and_subaccount_relation(
            pool.write_ref(),
            wallet_address,
            binding_address,
        )
        .await
    }

    pub async fn wallet_latest(
        pool: &ApiWalletDbPool,
    ) -> Result<Option<ApiWalletEntity>, crate::Error> {
        Ok(ApiWalletDao::wallet_latest(pool.read_ref()).await?)
    }

    // TODO: 想办法用ApiWalletDbPool替换executor
    pub async fn uid_list(pool: &ApiWalletDbPool) -> Result<Vec<(String,)>, crate::Error> {
        Ok(ApiWalletDao::uid_list(pool.read_ref()).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::ApiWalletRepo;
    use crate::{dao::api_wallet::ApiWalletDao, repositories::test_helper::setup_api_wallet_pool};

    #[tokio::test]
    async fn api_wallet_repo_upsert_and_find_success() {
        let pool = setup_api_wallet_pool("wallet_db_api_wallet_success").await;
        let uid = "uid_wallet_s_1";
        let address = "0xapi_wallet_s_1";

        ApiWalletRepo::upsert(
            &pool,
            uid,
            "wallet_name",
            address,
            "phrase",
            "seed",
            crate::entities::api_wallet::ApiWalletType::SubAccount,
            None,
            "sn_1",
        )
        .await
        .unwrap();

        let got = ApiWalletRepo::find_by_address(&pool, address).await.unwrap().unwrap();
        assert_eq!(got.address, address);
        assert_eq!(got.uid, uid);
        assert_eq!(got.name, "wallet_name");

        let by_uid = ApiWalletRepo::find_by_uid(&pool, uid).await.unwrap().unwrap();
        assert_eq!(by_uid.address, address);

        let list = ApiWalletRepo::list(
            &pool,
            Some(crate::entities::api_wallet::ApiWalletType::SubAccount),
        )
        .await
        .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].uid, uid);
    }

    #[tokio::test]
    async fn api_wallet_repo_missing_address_returns_none() {
        let pool = setup_api_wallet_pool("wallet_db_api_wallet_edge").await;
        let got = ApiWalletRepo::find_by_address(&pool, "0xapi_wallet_missing").await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn api_wallet_repo_tx_rollback_keeps_name_unchanged() {
        let pool = setup_api_wallet_pool("wallet_db_api_wallet_rollback").await;
        let address = "0xapi_wallet_rb_1";

        ApiWalletRepo::upsert(
            &pool,
            "uid_wallet_rb_1",
            "old_name",
            address,
            "phrase",
            "seed",
            crate::entities::api_wallet::ApiWalletType::SubAccount,
            None,
            "sn_rb_1",
        )
        .await
        .unwrap();

        let mut tx = pool.write_ref().begin().await.unwrap();
        let changed = ApiWalletDao::edit_name(tx.as_mut(), address, "new_name").await.unwrap();
        assert!(changed);
        tx.rollback().await.unwrap();

        let got = ApiWalletRepo::find_by_address(&pool, address).await.unwrap().unwrap();
        assert_eq!(got.name, "old_name");

        let by_uid = ApiWalletRepo::find_by_uid(&pool, "uid_wallet_rb_1").await.unwrap().unwrap();
        assert_eq!(by_uid.name, "old_name");
    }
}
