use crate::entities::{
    api_chain::{ApiChainCreateVo, ApiChainEntity, ApiChainWithNode},
    chain::ChainWithNode,
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

    pub async fn chain_node_info<'a, E>(
        exec: E,
        chain_code: &str,
    ) -> Result<Option<ApiChainWithNode>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
                            select q.*, a.rpc_url, a.ws_url, a.http_url, a.network, a.name as node_name
                            from chain as q
                            join node a on q.node_id = a.node_id
                            where q.chain_code = ? and q.status = 1;"#;
        sqlx::query_as::<sqlx::Sqlite, ApiChainWithNode>(sql)
            .bind(chain_code)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
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

    pub async fn upsert<'c, E>(executor: E, input: ApiChainCreateVo) -> Result<(), crate::Error>
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

        let rec = sqlx::query_as::<_, ApiChainEntity>(sql)
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

    pub async fn detail_with_main_symbol<'a, E>(
        exec: E,
        main_symbol: &str,
    ) -> Result<Option<ApiChainEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
                        SELECT *
                        FROM api_chain
                        WHERE main_symbol = $1 AND status = 1;"#;
        sqlx::query_as::<sqlx::Sqlite, ApiChainEntity>(sql)
            .bind(main_symbol)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn upsert_multi_chain<'a, E>(
        executor: E,
        input: Vec<ApiChainCreateVo>,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if input.is_empty() {
            return Ok(());
        }

        let mut query_builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "INSERT INTO api_chain (name, chain_code, protocols, status, main_symbol, created_at, updated_at) ",
        );

        query_builder.push_values(input, |mut b, chain| {
            let protocols =
                wallet_utils::serde_func::serde_to_string(&chain.protocols).unwrap_or_default();
            b.push_bind(chain.name.clone())
                .push_bind(chain.chain_code)
                .push_bind(protocols)
                .push_bind(chain.status)
                .push_bind(chain.main_symbol)
                .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
                .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')");
        });

        query_builder.push(
            " ON CONFLICT (chain_code)
              DO UPDATE SET
                  name = excluded.name,
                  status = excluded.status,
                  main_symbol = excluded.main_symbol,
                  updated_at = excluded.updated_at",
        );
        let query = query_builder.build();
        query.execute(executor).await.map_err(|e| crate::Error::Database(e.into()))?;
        Ok(())
    }

    pub async fn toggle_chains_status<'a, E>(
        executor: E,
        chain_codes: &[String],
    ) -> Result<Vec<ApiChainEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let chain_codes = crate::any_in_collection(chain_codes, "','");
        // 用逗号分隔的列表，用于 IN 查询
        let sql = format!(
            "UPDATE api_chain
            SET 
                status = CASE 
                            WHEN chain_code IN ('{}') THEN 1 
                            ELSE 0 
                        END,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            RETURNING *",
            chain_codes
        );

        // 为 `IN` 子句绑定参数，确保 chain_codes 被正确处理
        sqlx::query_as::<sqlx::Sqlite, ApiChainEntity>(&sql)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn set_chain_node_id_empty<'a, E>(
        executor: E,
        node_id: &str,
    ) -> Result<Vec<ApiChainEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            update api_chain set 
                node_id = null,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            where node_id = $1
            RETURNING *
            "#;

        sqlx::query_as::<sqlx::Sqlite, ApiChainEntity>(sql)
            .bind(node_id)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn set_api_chain_node<'a, E>(
        executor: E,
        chain_code: &str,
        node_id: &str,
    ) -> Result<Vec<ApiChainEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            update api_chain set 
                node_id = $2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            where chain_code = $1
            RETURNING *
            "#;

        sqlx::query_as::<sqlx::Sqlite, ApiChainEntity>(sql)
            .bind(chain_code)
            .bind(node_id)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list_with_node_info<'a, E>(exec: E) -> Result<Vec<ApiChainWithNode>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select q.*, a.rpc_url, a.ws_url, a.http_url, a.network, a.name as node_name
                            from api_chain as q  
                            left join node a on q.node_id = a.node_id WHERE q.status = 1;";

        sqlx::query_as::<sqlx::Sqlite, ChainWithNode>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
