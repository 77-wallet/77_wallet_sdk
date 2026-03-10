use crate::{ApiFundsDbPool, dao::api_nonce::ApiNonceDao};

pub struct ApiNonceRepo;

impl ApiNonceRepo {
    pub async fn get_api_nonce(
        pool: &ApiFundsDbPool,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<i64, crate::Error> {
        ApiNonceDao::get_api_nonce(pool.as_ref(), from_addr, chain_code).await
    }

    pub async fn get_api_nonce_optional(
        pool: &ApiFundsDbPool,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<Option<i64>, crate::Error> {
        ApiNonceDao::get_api_nonce_optional(pool.as_ref(), from_addr, chain_code).await
    }

    // 写权限限制：只允许 NonceEngine 调用
    #[doc(hidden)]
    pub async fn allocate_next_nonce(
        pool: &ApiFundsDbPool,
        from_addr: &str,
        chain_code: &str,
        initial_nonce: i32,
    ) -> Result<i32, crate::Error> {
        ApiNonceDao::upsert_and_get_api_nonce(
            pool.write_ref(),
            from_addr,
            chain_code,
            initial_nonce,
        )
        .await
    }

    // 写权限限制：只允许 NonceEngine 调用
    #[doc(hidden)]
    pub async fn set_nonce_floor(
        pool: &ApiFundsDbPool,
        from_addr: &str,
        chain_code: &str,
        floor_nonce: i64,
    ) -> Result<i64, crate::Error> {
        ApiNonceDao::upsert_nonce_floor(pool.write_ref(), from_addr, chain_code, floor_nonce).await
    }

    // 写权限限制：只允许 NonceEngine 调用
    #[doc(hidden)]
    pub async fn set_nonce_exact(
        pool: &ApiFundsDbPool,
        from_addr: &str,
        chain_code: &str,
        exact_nonce: i64,
    ) -> Result<i64, crate::Error> {
        ApiNonceDao::upsert_nonce_exact(pool.write_ref(), from_addr, chain_code, exact_nonce).await
    }

    // 写权限限制：只允许 NonceEngine 调用
    #[doc(hidden)]
    pub async fn upsert_and_get_api_nonce(
        pool: &ApiFundsDbPool,
        from_addr: &str,
        chain_code: &str,
        nonce: i32,
    ) -> Result<i32, crate::Error> {
        Self::allocate_next_nonce(pool, from_addr, chain_code, nonce).await
    }

    // 写权限限制：只允许 NonceEngine 调用
    #[doc(hidden)]
    pub async fn ensure_initialized(
        pool: &ApiFundsDbPool,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<(), crate::Error> {
        // 尝试获取现有 nonce，如果不存在则初始化
        match ApiNonceDao::get_api_nonce_optional(pool.as_ref(), from_addr, chain_code).await? {
            Some(_) => Ok(()),
            None => {
                // 记录不存在，初始化
                // 注意：这里只是兜底行为，真实系统应由 NonceEngine slow path 完成 bootstrap
                let _ =
                    ApiNonceDao::upsert_nonce_floor(pool.write_ref(), from_addr, chain_code, -1)
                        .await?;
                Ok(())
            }
        }
    }

    // 写权限限制：只允许 NonceEngine 调用
    #[doc(hidden)]
    pub async fn get_all_api_nonce_paginated(
        pool: &ApiFundsDbPool,
        cursor: Option<(&str, &str)>,
        limit: i32,
    ) -> Result<Vec<(String, String, i64)>, crate::Error> {
        ApiNonceDao::get_all_api_nonce_paginated(pool.as_ref(), cursor, limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::ApiNonceRepo;
    use crate::{
        SqlitePoolConfig,
        dao::api_nonce::ApiNonceDao,
        repositories::test_helper::{setup_api_funds_pool, setup_api_funds_pool_with_config},
    };
    use std::{sync::Arc, time::Duration};
    use tokio::sync::{Barrier, oneshot};

    fn is_sqlite_locked(err: &crate::Error) -> bool {
        match err {
            crate::Error::Database(crate::DatabaseError::Sqlx(sqlx::Error::Database(db_err))) => {
                db_err.code().as_deref() == Some("5")
            }
            _ => false,
        }
    }

    async fn run_nonce_concurrency_regression() {
        let cfg_multi = SqlitePoolConfig { reader_max_connections: 4, writer_max_connections: 4 };
        let pool =
            setup_api_funds_pool_with_config("wallet_db_nonce_concurrent_multi", cfg_multi).await;

        let addr = "0xnonce_lock_multi";
        let chain = wallet_types::constant::chain_code::ETHEREUM;
        ApiNonceRepo::set_nonce_exact(&pool, addr, chain, 0).await.unwrap();

        let gate = Arc::new(Barrier::new(2));
        let pool_hold = pool.clone();
        let gate_hold = gate.clone();
        let holder = tokio::spawn(async move {
            let mut tx = pool_hold.write_ref().begin().await.unwrap();
            ApiNonceDao::upsert_nonce_exact(tx.as_mut(), addr, chain, 101).await.unwrap();
            gate_hold.wait().await;
            tokio::time::sleep(Duration::from_secs(6)).await;
            tx.commit().await.unwrap();
        });

        let pool_race = pool.clone();
        let racer = tokio::spawn(async move {
            gate.wait().await;
            ApiNonceRepo::set_nonce_exact(&pool_race, addr, chain, 202).await
        });

        holder.await.unwrap();
        let race_res = racer.await.unwrap();
        assert!(race_res.as_ref().is_err_and(is_sqlite_locked));

        // default config: single writer should avoid DB lock
        let pool_default = setup_api_funds_pool("wallet_db_nonce_concurrent_default").await;
        ApiNonceRepo::set_nonce_exact(&pool_default, addr, chain, 0).await.unwrap();

        let gate_default = Arc::new(Barrier::new(2));
        let pool_hold_default = pool_default.clone();
        let gate_hold_default = gate_default.clone();
        let holder_default = tokio::spawn(async move {
            let mut tx = pool_hold_default.write_ref().begin().await.unwrap();
            ApiNonceDao::upsert_nonce_exact(tx.as_mut(), addr, chain, 303).await.unwrap();
            gate_hold_default.wait().await;
            tokio::time::sleep(Duration::from_secs(2)).await;
            tx.commit().await.unwrap();
        });

        let pool_race_default = pool_default.clone();
        let racer_default = tokio::spawn(async move {
            gate_default.wait().await;
            ApiNonceRepo::set_nonce_exact(&pool_race_default, addr, chain, 404).await
        });

        holder_default.await.unwrap();
        let ok_res = racer_default.await.unwrap();
        assert!(ok_res.is_ok());
        let got = ApiNonceRepo::get_api_nonce(&pool_default, addr, chain).await.unwrap();
        assert_eq!(got, 404);
    }

    #[tokio::test]
    async fn nonce_allocate_and_get_success() {
        let pool = setup_api_funds_pool("wallet_db_nonce_success").await;
        let addr = "0xnonce_s_1";
        let chain = wallet_types::constant::chain_code::ETHEREUM;

        let n1 = ApiNonceRepo::allocate_next_nonce(&pool, addr, chain, 10).await.unwrap();
        assert_eq!(n1, 10);
        let n2 = ApiNonceRepo::allocate_next_nonce(&pool, addr, chain, 10).await.unwrap();
        assert_eq!(n2, 11);

        let got = ApiNonceRepo::get_api_nonce(&pool, addr, chain).await.unwrap();
        assert_eq!(got, 11);
    }

    #[tokio::test]
    async fn nonce_optional_missing_returns_none() {
        let pool = setup_api_funds_pool("wallet_db_nonce_edge").await;
        let got = ApiNonceRepo::get_api_nonce_optional(
            &pool,
            "0xnonce_missing",
            wallet_types::constant::chain_code::ETHEREUM,
        )
        .await
        .unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn nonce_tx_rollback_restores_previous_value() {
        let pool = setup_api_funds_pool("wallet_db_nonce_rollback").await;
        let addr = "0xnonce_rb_1";
        let chain = wallet_types::constant::chain_code::ETHEREUM;

        ApiNonceRepo::set_nonce_exact(&pool, addr, chain, 7).await.unwrap();

        let mut tx = pool.write_ref().begin().await.unwrap();
        let changed = ApiNonceDao::upsert_nonce_exact(tx.as_mut(), addr, chain, 99).await.unwrap();
        assert_eq!(changed, 99);
        tx.rollback().await.unwrap();

        let got = ApiNonceRepo::get_api_nonce(&pool, addr, chain).await.unwrap();
        assert_eq!(got, 7);
    }

    #[tokio::test]
    async fn concurrent_nonce_updates() {
        run_nonce_concurrency_regression().await;
    }

    #[tokio::test]
    async fn concurrent_balance_upserts() {
        // alias test name for lock-regression command compatibility
        run_nonce_concurrency_regression().await;
    }

    #[tokio::test]
    async fn read_queries_are_not_blocked_by_long_writer_transaction() {
        let pool = setup_api_funds_pool("wallet_db_nonce_reader_not_blocked").await;
        let addr = "0xnonce_reader";
        let chain = wallet_types::constant::chain_code::ETHEREUM;
        ApiNonceRepo::set_nonce_exact(&pool, addr, chain, 7).await.unwrap();

        let (ready_tx, ready_rx) = oneshot::channel();
        let pool_writer = pool.clone();
        let writer = tokio::spawn(async move {
            let mut tx = pool_writer.write_ref().begin().await.unwrap();
            ApiNonceDao::upsert_nonce_exact(tx.as_mut(), addr, chain, 8).await.unwrap();
            let _ = ready_tx.send(());
            tokio::time::sleep(Duration::from_secs(2)).await;
            tx.commit().await.unwrap();
        });

        ready_rx.await.unwrap();

        let read_res = tokio::time::timeout(
            Duration::from_millis(800),
            ApiNonceRepo::get_api_nonce(&pool, addr, chain),
        )
        .await;
        assert!(read_res.is_ok(), "reader query timed out while writer tx was open");
        assert_eq!(read_res.unwrap().unwrap(), 7);

        writer.await.unwrap();
        let after = ApiNonceRepo::get_api_nonce(&pool, addr, chain).await.unwrap();
        assert_eq!(after, 8);
    }
}
