use crate::{
    ApiWalletDbPool, dao::api_collect_strategy::ApiCollectStrategyDao,
    entities::api_collect_strategy::ApiCollectStrategyEntity,
};
pub struct ApiCollectStrategyRepo;

impl ApiCollectStrategyRepo {
    pub async fn list_api_collect_strategy(
        pool: &ApiWalletDbPool,
    ) -> Result<Vec<ApiCollectStrategyEntity>, crate::Error> {
        ApiCollectStrategyDao::all_api_collect_strategy(pool.as_ref()).await
    }

    pub async fn page_api_collect_strategy(
        pool: &ApiWalletDbPool,
        page: i64,
        page_size: i64,
    ) -> Result<(i64, Vec<ApiCollectStrategyEntity>), crate::Error> {
        ApiCollectStrategyDao::page_api_collect_strategy(pool.as_ref(), page, page_size).await
    }

    pub async fn upsert(
        pool: &ApiWalletDbPool,
        input: ApiCollectStrategyEntity,
    ) -> Result<(), crate::Error> {
        ApiCollectStrategyDao::upsert(pool.as_ref(), input).await
    }

    pub async fn get_by_uid(
        pool: &ApiWalletDbPool,
        uid: &str,
    ) -> Result<Option<ApiCollectStrategyEntity>, crate::Error> {
        ApiCollectStrategyDao::get_by_uid(pool.as_ref(), uid).await
    }

    pub async fn delete(pool: &ApiWalletDbPool, uid: &str) -> Result<(), crate::Error> {
        ApiCollectStrategyDao::delete(pool.as_ref(), uid).await
    }
}
