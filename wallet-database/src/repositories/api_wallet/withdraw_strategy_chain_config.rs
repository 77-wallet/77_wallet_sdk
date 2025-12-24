use crate::{
    DbPool, dao::api_withdraw_strategy_chain_config::ApiWithdrawStrategyChainConfigDao,
    entities::api_withdraw_strategy_chain_config::ApiWithdrawStrategyChainConfigEntity,
};
pub struct ApiWithdrawStrategyChainConfigRepo;

impl ApiWithdrawStrategyChainConfigRepo {
    pub async fn upsert(
        pool: &DbPool,
        input: ApiWithdrawStrategyChainConfigEntity,
    ) -> Result<(), crate::Error> {
        ApiWithdrawStrategyChainConfigDao::upsert(pool.as_ref(), input).await
    }

    pub async fn get_by_strategy_id(
        pool: &DbPool,
        strategy_id: i64,
    ) -> Result<Vec<ApiWithdrawStrategyChainConfigEntity>, crate::Error> {
        ApiWithdrawStrategyChainConfigDao::get_chain_configs_by_strategy_id(
            pool.as_ref(),
            strategy_id,
        )
        .await
    }

    pub async fn delete_by_strategy_id(
        pool: &DbPool,
        strategy_id: i64,
    ) -> Result<(), crate::Error> {
        ApiWithdrawStrategyChainConfigDao::delete_chain_configs_by_strategy_id(
            pool.as_ref(),
            strategy_id,
        )
        .await
    }

    pub async fn delete_chain_config(
        pool: &DbPool,
        strategy_id: i64,
        chain_code: &str,
    ) -> Result<(), crate::Error> {
        ApiWithdrawStrategyChainConfigDao::delete_chain_config(
            pool.as_ref(),
            strategy_id,
            chain_code,
        )
        .await
    }
}
