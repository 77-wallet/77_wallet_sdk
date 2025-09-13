use crate::entities::api_withdraw_strategy::ApiWithdrawStrategyEntity;
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiWithdrawStrategyDao;

impl ApiWithdrawStrategyDao {
    pub async fn all_api_withdraw_strategy<'a, E>(exec: E) -> Result<Vec<ApiWithdrawStrategyEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"SELECT * FROM api_withdraw_strategy"#;
        let result = sqlx::query_as::<_, ApiWithdrawStrategyEntity>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    pub async fn page_api_withdraw_strategy<'a, E>(
        exec: E,
        page: i64,
        page_size: i64,
    ) -> Result<(i64, Vec<ApiWithdrawStrategyEntity>), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Clone,
    {
        let count_sql = "SELECT count(*) FROM api_withdraw_strategy";
        let count = sqlx::query_scalar::<_, i64>(count_sql)
            .fetch_one(exec.clone())
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        let sql = "SELECT * FROM api_withdraw_strategy ORDER BY created_at DESC LIMIT ? OFFSET ?";
        let res = sqlx::query_as::<_, ApiWithdrawStrategyEntity>(sql)
            .bind(page_size)
            .bind(page)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok((count, res))
    }

    async fn upsert<'c, E>(executor: E, input: ApiWithdrawStrategyEntity) -> Result<(), crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"
            Insert into api_withdraw_strategy
                (id,uid,name,min_value,idx,risk_idx,created_at,updated_at)
            values
                ($1, $2, $3, $4, $5, $6, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            on conflict (uid)
            do update set
                min_value = excluded.min_value,
                idx = excluded.idx,
                risk_idx = excluded.risk_idx,
                updated_at = excluded.updated_at
            returning *
        "#;

        let mut rec = sqlx::query_as::<_, ApiWithdrawStrategyEntity>(sql)
            .bind(&input.uid)
            .bind(&input.name)
            .bind(&input.min_value)
            .bind(&input.idx)
            .bind(&input.risk_idx)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }
}