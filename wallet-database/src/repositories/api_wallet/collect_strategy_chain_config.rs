use crate::{
    DbPool,
    dao::api_collect_strategy_chain_config::ApiCollectStrategyChainConfigDao,
    entities::api_collect_strategy_chain_config::ApiCollectStrategyChainConfigEntity,
};
pub struct ApiCollectStrategyChainConfigRepo;

impl ApiCollectStrategyChainConfigRepo {
    pub async fn upsert(
        pool: &DbPool,
        input: ApiCollectStrategyChainConfigEntity,
    ) -> Result<(), crate::Error> {
        ApiCollectStrategyChainConfigDao::upsert(pool.as_ref(), input).await
    }

    pub async fn get_by_strategy_id(
        pool: &DbPool,
        strategy_id: i64,
    ) -> Result<Vec<ApiCollectStrategyChainConfigEntity>, crate::Error> {
        ApiCollectStrategyChainConfigDao::get_chain_configs_by_strategy_id(pool.as_ref(), strategy_id).await
    }

    pub async fn delete_by_strategy_id(
        pool: &DbPool,
        strategy_id: i64,
    ) -> Result<(), crate::Error> {
        ApiCollectStrategyChainConfigDao::delete_chain_configs_by_strategy_id(pool.as_ref(), strategy_id).await
    }

    pub async fn delete_chain_config(
        pool: &DbPool,
        strategy_id: i64,
        chain_code: &str,
    ) -> Result<(), crate::Error> {
        ApiCollectStrategyChainConfigDao::delete_chain_config(pool.as_ref(), strategy_id, chain_code).await
    }
}