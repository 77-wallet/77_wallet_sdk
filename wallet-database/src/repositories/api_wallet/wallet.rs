use crate::{
    ApiWalletDbPool,
    dao::api_wallet::ApiWalletDao,
    db::sqlite_retry::with_sqlite_locked_retry,
    entities::api_wallet::{ApiWalletEntity, ApiWalletType},
};

pub struct ApiWalletRepo;

impl ApiWalletRepo {
    async fn with_write_guard<T, F, Fut>(
        pool: &ApiWalletDbPool,
        op: &'static str,
        action: F,
    ) -> Result<T, crate::Error>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, crate::Error>>,
    {
        let _write_guard = pool.lock_write_with_metric(op).await;
        let tx_start = std::time::Instant::now();
        let result = with_sqlite_locked_retry(action).await;
        let elapsed_ms = tx_start.elapsed().as_secs_f64() * 1000.0;
        tracing::info!(
            metric = "write_tx_duration_ms",
            db = "api_wallet.db",
            op,
            value_ms = %elapsed_ms,
            ok = %result.is_ok(),
            "api wallet write finished"
        );
        result
    }

    pub async fn upsert(
        pool: &ApiWalletDbPool,
        uid: &str,
        name: &str,
        address: &str,
        phrase: &[u8],
        seed: &[u8],
        wallet_type: ApiWalletType,
        binding_address: Option<&str>,
        sn: &str,
        import_stage: u8,
    ) -> Result<ApiWalletEntity, crate::Error> {
        Self::with_write_guard(pool, "upsert_api_wallet", || async {
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
                import_stage,
            )
            .await?)
        })
        .await
    }

    pub async fn edit_name(
        pool: &ApiWalletDbPool,
        address: &str,
        name: &str,
    ) -> Result<bool, crate::Error> {
        Self::with_write_guard(pool, "edit_api_wallet_name", || async {
            Ok(ApiWalletDao::edit_name(pool.write_ref(), address, name).await?)
        })
        .await
    }

    pub async fn update_merchant_id(
        pool: &ApiWalletDbPool,
        address: &str,
        merchant_id: &str,
    ) -> Result<bool, crate::Error> {
        Self::with_write_guard(pool, "update_merchant_id", || async {
            Ok(ApiWalletDao::update_merchain_id(pool.write_ref(), address, merchant_id).await?)
        })
        .await
    }

    pub async fn update_app_id(
        pool: &ApiWalletDbPool,
        address: &str,
        app_id: Option<&str>,
    ) -> Result<bool, crate::Error> {
        Self::with_write_guard(pool, "update_app_id", || async {
            Ok(ApiWalletDao::update_app_id(pool.write_ref(), address, app_id).await?)
        })
        .await
    }

    pub async fn update_sn(
        pool: &ApiWalletDbPool,
        address: &str,
        sn: &str,
    ) -> Result<bool, crate::Error> {
        Self::with_write_guard(pool, "update_wallet_sn", || async {
            Ok(ApiWalletDao::update_sn(pool.write_ref(), address, sn).await?)
        })
        .await
    }

    pub async fn update_seed_and_phrase(
        pool: &ApiWalletDbPool,
        uid: &str,
        phrase: &[u8],
        seed: &[u8],
    ) -> Result<bool, crate::Error> {
        Self::with_write_guard(pool, "update_seed_and_phrase", || async {
            Ok(ApiWalletDao::update_seed_and_phrase(pool.write_ref(), uid, phrase, seed).await?)
        })
        .await
    }

    pub async fn unbind_uid(pool: &ApiWalletDbPool, address: &str) -> Result<bool, crate::Error> {
        Self::with_write_guard(pool, "unbind_wallet_uid", || async {
            Ok(ApiWalletDao::unbind_uid(pool.write_ref(), address).await?)
        })
        .await
    }

    pub async fn mark_init(pool: &ApiWalletDbPool, uid: &str) -> Result<bool, crate::Error> {
        Self::with_write_guard(pool, "mark_wallet_init", || async {
            Ok(ApiWalletDao::mark_init(pool.write_ref(), uid).await?)
        })
        .await
    }

    pub async fn update_import_stage(
        pool: &ApiWalletDbPool,
        uid: &str,
        import_stage: u8,
    ) -> Result<bool, crate::Error> {
        Self::with_write_guard(pool, "update_api_wallet_import_stage", || async {
            Ok(ApiWalletDao::update_import_stage(pool.write_ref(), uid, import_stage).await?)
        })
        .await
    }

    pub async fn physical_delete(
        pool: &ApiWalletDbPool,
        wallet_addresses: &[&str],
    ) -> Result<Vec<ApiWalletEntity>, crate::Error> {
        Self::with_write_guard(pool, "delete_api_wallets", || async {
            Ok(ApiWalletDao::physical_delete(pool.write_ref(), wallet_addresses).await?)
        })
        .await
    }

    pub async fn physical_delete_all_wallet(pool: &ApiWalletDbPool) -> Result<u64, crate::Error> {
        Self::with_write_guard(pool, "delete_all_api_wallets", || async {
            Ok(ApiWalletDao::physical_delete_all_wallet(pool.write_ref()).await?)
        })
        .await
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
        Self::with_write_guard(pool, "bind_withdraw_relation", || async {
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
        })
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
        let seed = b"seed".to_vec();

        ApiWalletRepo::upsert(
            &pool,
            uid,
            "wallet_name",
            address,
            b"phrase",
            &seed,
            crate::entities::api_wallet::ApiWalletType::SubAccount,
            None,
            "sn_1",
            0,
        )
        .await
        .unwrap();

        let got = ApiWalletRepo::find_by_address(&pool, address).await.unwrap().unwrap();
        assert_eq!(got.address, address);
        assert_eq!(got.uid, uid);
        assert_eq!(got.name, "wallet_name");
        assert_eq!(got.phrase, b"phrase".to_vec());
        assert_eq!(got.seed, seed);

        let by_uid = ApiWalletRepo::find_by_uid(&pool, uid).await.unwrap().unwrap();
        assert_eq!(by_uid.address, address);
        assert_eq!(by_uid.seed, seed);

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
        let seed = b"seed".to_vec();

        ApiWalletRepo::upsert(
            &pool,
            "uid_wallet_rb_1",
            "old_name",
            address,
            b"phrase",
            &seed,
            crate::entities::api_wallet::ApiWalletType::SubAccount,
            None,
            "sn_rb_1",
            0,
        )
        .await
        .unwrap();

        let mut tx = pool.write_ref().begin().await.unwrap();
        let changed = ApiWalletDao::edit_name(tx.as_mut(), address, "new_name").await.unwrap();
        assert!(changed);
        tx.rollback().await.unwrap();

        let got = ApiWalletRepo::find_by_address(&pool, address).await.unwrap().unwrap();
        assert_eq!(got.name, "old_name");
        assert_eq!(got.seed, seed);

        let by_uid = ApiWalletRepo::find_by_uid(&pool, "uid_wallet_rb_1").await.unwrap().unwrap();
        assert_eq!(by_uid.name, "old_name");
        assert_eq!(by_uid.seed, seed);
    }

    #[tokio::test]
    async fn api_wallet_repo_upsert_roundtrip_and_storage() {
        let pool = setup_api_wallet_pool("wallet_db_api_wallet_success").await;
        let uid = "uid_wallet_1";
        let address = "0xapi_wallet_1";
        let seed = vec![0x11, 0x22, 0x33, 0x44, 0xff, 0x00, 0x99];

        ApiWalletRepo::upsert(
            &pool,
            uid,
            "wallet_name",
            address,
            b"phrase",
            &seed,
            crate::entities::api_wallet::ApiWalletType::SubAccount,
            None,
            "sn_1",
            0,
        )
        .await
        .unwrap();

        let got = ApiWalletRepo::find_by_address(&pool, address).await.unwrap().unwrap();
        assert_eq!(got.address, address);
        assert_eq!(got.uid, uid);
        assert_eq!(got.seed, seed);

        let by_uid = ApiWalletRepo::find_by_uid(&pool, uid).await.unwrap().unwrap();
        assert_eq!(by_uid.address, address);
        assert_eq!(by_uid.seed, seed);

        let list = ApiWalletRepo::list(
            &pool,
            Some(crate::entities::api_wallet::ApiWalletType::SubAccount),
        )
        .await
        .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].uid, uid);

        let latest = ApiWalletRepo::wallet_latest(&pool).await.unwrap().unwrap();
        assert_eq!(latest.uid, uid);

        let typeof_seed =
            sqlx::query_as::<_, (String,)>("SELECT typeof(seed) FROM api_wallet WHERE address = ?")
                .bind(address)
                .fetch_one(pool.read_ref())
                .await
                .unwrap();
        assert_eq!(typeof_seed.0, "blob");

        let typeof_phrase = sqlx::query_as::<_, (String,)>(
            "SELECT typeof(phrase) FROM api_wallet WHERE address = ?",
        )
        .bind(address)
        .fetch_one(pool.read_ref())
        .await
        .unwrap();
        assert_eq!(typeof_phrase.0, "blob");
    }

    #[tokio::test]
    async fn api_wallet_repo_update_seed_missing_uid_returns_false() {
        let pool = setup_api_wallet_pool("wallet_db_api_wallet_missing_uid").await;
        let uid = "uid_wallet_missing";
        let address = "0xapi_wallet_missing";
        let initial_seed = vec![0xaa, 0xbb, 0xcc];

        ApiWalletRepo::upsert(
            &pool,
            uid,
            "wallet_missing_name",
            address,
            b"phrase",
            &initial_seed,
            crate::entities::api_wallet::ApiWalletType::SubAccount,
            None,
            "sn_missing",
            0,
        )
        .await
        .unwrap();

        let changed =
            ApiWalletRepo::update_seed_and_phrase(&pool, "missing_uid", b"new_phrase", b"new_seed")
                .await
                .unwrap();
        assert!(!changed);

        let got = ApiWalletRepo::find_by_address(&pool, address).await.unwrap().unwrap();
        assert_eq!(got.seed, initial_seed);
        assert_eq!(got.phrase, b"phrase".to_vec());
    }

    #[tokio::test]
    async fn api_wallet_repo_wallet_latest_returns_a_single_row() {
        let pool = setup_api_wallet_pool("wallet_db_api_wallet_latest").await;
        let seed = vec![0xaa, 0xbb, 0xcc];
        use std::{thread::sleep, time::Duration};

        ApiWalletRepo::upsert(
            &pool,
            "uid_wallet_latest_1",
            "wallet_latest_name_1",
            "0xapi_wallet_latest_1",
            b"phrase_1",
            &seed,
            crate::entities::api_wallet::ApiWalletType::SubAccount,
            None,
            "sn_latest",
            0,
        )
        .await
        .unwrap();

        sleep(Duration::from_secs(1));

        ApiWalletRepo::upsert(
            &pool,
            "uid_wallet_latest_2",
            "wallet_latest_name_2",
            "0xapi_wallet_latest_2",
            b"phrase_2",
            &seed,
            crate::entities::api_wallet::ApiWalletType::SubAccount,
            None,
            "sn_latest",
            0,
        )
        .await
        .unwrap();

        let latest = ApiWalletRepo::wallet_latest(&pool).await.unwrap().unwrap();
        assert_eq!(latest.address, "0xapi_wallet_latest_2");
    }
}
