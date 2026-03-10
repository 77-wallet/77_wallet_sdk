use crate::{
    ApiWalletDbPool, dao::api_withdraw_strategy::ApiWithdrawStrategyDao,
    entities::api_withdraw_strategy::ApiWithdrawStrategyEntity,
};
pub struct ApiWithdrawStrategyRepo;

impl ApiWithdrawStrategyRepo {
    pub async fn list_api_withdraw_strategy(
        pool: &ApiWalletDbPool,
    ) -> Result<Vec<ApiWithdrawStrategyEntity>, crate::Error> {
        ApiWithdrawStrategyDao::all_api_withdraw_strategy(pool.as_ref()).await
    }

    pub async fn page_api_withdraw_strategy(
        pool: &ApiWalletDbPool,
        page: i64,
        page_size: i64,
    ) -> Result<(i64, Vec<ApiWithdrawStrategyEntity>), crate::Error> {
        ApiWithdrawStrategyDao::page_api_withdraw_strategy(pool.as_ref(), page, page_size).await
    }

    pub async fn upsert(
        pool: &ApiWalletDbPool,
        input: ApiWithdrawStrategyEntity,
    ) -> Result<(), crate::Error> {
        ApiWithdrawStrategyDao::upsert(pool.as_ref(), input).await
    }

    pub async fn get_by_uid(
        pool: &ApiWalletDbPool,
        uid: &str,
    ) -> Result<Option<ApiWithdrawStrategyEntity>, crate::Error> {
        ApiWithdrawStrategyDao::get_by_uid(pool.as_ref(), uid).await
    }

    pub async fn delete(pool: &ApiWalletDbPool, uid: &str) -> Result<(), crate::Error> {
        ApiWithdrawStrategyDao::delete(pool.as_ref(), uid).await
    }
}

#[cfg(test)]
mod tests {
    use super::ApiWithdrawStrategyRepo;
    use crate::{
        dao::api_withdraw_strategy::ApiWithdrawStrategyDao,
        entities::api_withdraw_strategy::ApiWithdrawStrategyEntity,
        repositories::test_helper::setup_api_wallet_pool,
    };

    fn make_strategy(uid: &str, threshold: i32) -> ApiWithdrawStrategyEntity {
        ApiWithdrawStrategyEntity {
            id: 0,
            uid: uid.to_string(),
            threshold,
            created_at: Default::default(),
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn withdraw_strategy_repo_upsert_and_get_success() {
        let pool = setup_api_wallet_pool("wallet_db_withdraw_strategy_success").await;
        let uid = "withdraw_strategy_uid_s";
        ApiWithdrawStrategyRepo::upsert(&pool, make_strategy(uid, 30)).await.unwrap();

        let got = ApiWithdrawStrategyRepo::get_by_uid(&pool, uid).await.unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().threshold, 30);
    }

    #[tokio::test]
    async fn withdraw_strategy_repo_missing_uid_returns_none() {
        let pool = setup_api_wallet_pool("wallet_db_withdraw_strategy_edge").await;
        let got = ApiWithdrawStrategyRepo::get_by_uid(&pool, "withdraw_strategy_uid_missing")
            .await
            .unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn withdraw_strategy_repo_tx_rollback_keeps_threshold_unchanged() {
        let pool = setup_api_wallet_pool("wallet_db_withdraw_strategy_rollback").await;
        let uid = "withdraw_strategy_uid_rb";
        ApiWithdrawStrategyRepo::upsert(&pool, make_strategy(uid, 15)).await.unwrap();

        let mut tx = pool.as_ref().begin().await.unwrap();
        ApiWithdrawStrategyDao::upsert(tx.as_mut(), make_strategy(uid, 77)).await.unwrap();
        tx.rollback().await.unwrap();

        let got = ApiWithdrawStrategyRepo::get_by_uid(&pool, uid).await.unwrap().unwrap();
        assert_eq!(got.threshold, 15);
    }
}
