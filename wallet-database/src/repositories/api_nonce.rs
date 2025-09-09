use crate::{DbPool, dao::api_nonce::ApiNonceDao};

pub struct ApiNonceRepo;

impl ApiNonceRepo {

    pub async fn get_api_nonce(
        pool: &DbPool,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<i32, crate::Error> {
        ApiNonceDao::get_api_nonce(pool.as_ref(), from_addr, chain_code).await
    }

    pub async fn upsert_and_get_api_nonce(
        pool: &DbPool,
        from_addr: &str,
        chain_code: &str,
        nonce: i32,
    ) -> Result<i32, crate::Error> {
        ApiNonceDao::upsert_and_get_api_nonce(pool.as_ref(), from_addr, chain_code, nonce).await
    }
}
