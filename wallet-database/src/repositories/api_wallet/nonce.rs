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
        ApiNonceDao::upsert_and_get_api_nonce(pool.as_ref(), from_addr, chain_code, initial_nonce)
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
        ApiNonceDao::upsert_nonce_floor(pool.as_ref(), from_addr, chain_code, floor_nonce).await
    }

    // 写权限限制：只允许 NonceEngine 调用
    #[doc(hidden)]
    pub async fn set_nonce_exact(
        pool: &ApiFundsDbPool,
        from_addr: &str,
        chain_code: &str,
        exact_nonce: i64,
    ) -> Result<i64, crate::Error> {
        ApiNonceDao::upsert_nonce_exact(pool.as_ref(), from_addr, chain_code, exact_nonce).await
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
                let _ = ApiNonceDao::upsert_nonce_floor(pool.as_ref(), from_addr, chain_code, -1)
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
