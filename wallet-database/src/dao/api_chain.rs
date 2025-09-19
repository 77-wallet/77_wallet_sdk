use crate::entities::{
    api_chain::ApiChainEntity,
    chain::{ChainCreateVo, ChainEntity},
};
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiChainDao;

impl ApiChainDao {
    pub async fn list<'a, E>(
        exec: E,
        status: Option<u8>,
    ) -> Result<Vec<ApiChainEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = "SELECT * FROM api_chain".to_string();
        let mut conditions = Vec::new();

        if status.is_some() {
            conditions.push("status = ?".to_string());
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        let mut query = sqlx::query_as::<_, ApiChainEntity>(&sql);

        if let Some(status) = status {
            query = query.bind(status);
        }
        query.fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn detail<'a, E>(
        exec: E,
        chain_code: &str,
    ) -> Result<Option<ApiChainEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
                        SELECT *
                        FROM api_chain
                        WHERE chain_code = $1 AND status = 1;"#;
        sqlx::query_as::<sqlx::Sqlite, ApiChainEntity>(sql)
            .bind(chain_code)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn upsert<'c, E>(executor: E, input: ChainCreateVo) -> Result<(), crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"Insert into api_chain
            (name, chain_code, protocols, main_symbol, status, created_at, updated_at)
                values ($1, $2, $3, $4, $5, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                on conflict (chain_code)
                do update set
                    status = excluded.status,
                    name = excluded.name,
                    protocols = excluded.protocols,
                    updated_at = excluded.updated_at"#;
        let protocols = wallet_utils::serde_func::serde_to_string(&input.protocols)?;

        let mut rec = sqlx::query_as::<_, ChainEntity>(sql)
            .bind(&input.name)
            .bind(&input.chain_code)
            .bind(protocols)
            .bind(&input.main_symbol)
            .bind(input.status)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn set_chain_node<'a, E>(
        executor: E,
        chain_code: &str,
        node_id: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            update api_chain set
                node_id = $2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            where chain_code = $1
            "#;

        sqlx::query(sql)
            .bind(chain_code)
            .bind(node_id)
            .execute(executor)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
