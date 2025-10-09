use crate::{
    DbPool, dao::api_collect_strategy::ApiCollectStrategyDao,
    entities::api_collect_strategy::ApiCollectStrategyEntity,
};
pub struct ApiCollectStrategyRepo;

impl ApiCollectStrategyRepo {
    pub async fn list_api_collect_strategy(
        pool: &DbPool,
    ) -> Result<Vec<ApiCollectStrategyEntity>, crate::Error> {
        ApiCollectStrategyDao::all_api_collect_strategy(pool.as_ref()).await
    }
}
