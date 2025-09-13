use crate::{
    DbPool, dao::api_withdraw_strategy::ApiWithdrawStrategyDao,
    entities::api_withdraw_strategy::ApiWithdrawStrategyEntity,
};
pub struct ApiWithdrawStrategyRepo;

impl ApiWithdrawStrategyRepo {
    pub async fn list_api_withdraw_strategy(
        pool: &DbPool,
    ) -> Result<Vec<ApiWithdrawStrategyEntity>, crate::Error> {
        ApiWithdrawStrategyDao::all_api_withdraw_strategy(pool.as_ref()).await
    }
}
