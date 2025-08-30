use crate::dao::api_nonce::ApiNonceDao;
use crate::DbPool;

pub struct ApiNonceRepo;

impl ApiNonceRepo{


    pub async fn upsert_and_get_api_nonce(
        pool: &DbPool,
        uid: &str,
        name: &str,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<i32, crate::Error> {
        ApiNonceDao::upsert_and_get_api_nonce(pool.as_ref(), uid, name, from_addr, chain_code).await
    }
}