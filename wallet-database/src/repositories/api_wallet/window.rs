use crate::{ApiWalletDbPool, dao::api_window::ApiWindowDao};

pub struct ApiWindowRepo;

impl ApiWindowRepo {
    pub async fn get_api_offset(pool: &ApiWalletDbPool, id: i64) -> Result<i64, crate::Error> {
        ApiWindowDao::get_api_offset(pool.as_ref(), id).await
    }

    pub async fn upsert_api_offset(
        pool: &ApiWalletDbPool,
        id: i64,
        offset: i64,
    ) -> Result<(), crate::Error> {
        ApiWindowDao::upsert_api_offset(pool.as_ref(), id, offset).await
    }
}

#[cfg(test)]
mod tests {
    use super::ApiWindowRepo;
    use crate::{dao::api_window::ApiWindowDao, repositories::test_helper::setup_api_wallet_pool};

    #[tokio::test]
    async fn window_repo_upsert_and_get_success() {
        let pool = setup_api_wallet_pool("wallet_db_api_window_success").await;
        ApiWindowRepo::upsert_api_offset(&pool, 1, 42).await.unwrap();
        let got = ApiWindowRepo::get_api_offset(&pool, 1).await.unwrap();
        assert_eq!(got, 42);
    }

    #[tokio::test]
    async fn window_repo_missing_id_returns_zero() {
        let pool = setup_api_wallet_pool("wallet_db_api_window_edge").await;
        let got = ApiWindowRepo::get_api_offset(&pool, 999).await.unwrap();
        assert_eq!(got, 0);
    }

    #[tokio::test]
    async fn window_repo_tx_rollback_keeps_offset_unchanged() {
        let pool = setup_api_wallet_pool("wallet_db_api_window_rollback").await;
        ApiWindowRepo::upsert_api_offset(&pool, 7, 11).await.unwrap();

        let mut tx = pool.as_ref().begin().await.unwrap();
        ApiWindowDao::upsert_api_offset(tx.as_mut(), 7, 88).await.unwrap();
        tx.rollback().await.unwrap();

        let got = ApiWindowRepo::get_api_offset(&pool, 7).await.unwrap();
        assert_eq!(got, 11);
    }
}
