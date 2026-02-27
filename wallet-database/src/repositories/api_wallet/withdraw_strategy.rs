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
