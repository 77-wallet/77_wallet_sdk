use crate::entities::api_collect_strategy::ApiCollectStrategyEntity;
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiCollectStrategyDao;

impl ApiCollectStrategyDao {
    pub async fn all_api_collect_strategy<'a, E>(
        exec: E,
    ) -> Result<Vec<ApiCollectStrategyEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"SELECT * FROM api_collect_strategy"#;
        let result = sqlx::query_as::<_, ApiCollectStrategyEntity>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }
}
