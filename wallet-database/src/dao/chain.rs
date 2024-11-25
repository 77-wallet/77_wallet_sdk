use sqlx::{Executor, Sqlite};

use crate::{
    entities::chain::{ChainCreateVo, ChainEntity},
    sqlite::logic::chain::ChainWithNode,
};

// pub trait ChainDao{
//     async fn upsert()
// }
// #[async_trait]

pub struct Id {
    pub chain_code: String,
    pub node_id: String,
}

pub struct ChainDao;

impl ChainDao {
    pub async fn upsert<'c, E>(
        executor: E,
        input: ChainCreateVo,
    ) -> Result<ChainEntity, crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"Insert into chain 
            (name, chain_code, node_id, protocols, main_symbol, status, created_at, updated_at)
                values ($1, $2, $3, $4, $5, $6, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                on conflict (chain_code)
                do update set
                    updated_at = excluded.updated_at
                returning *"#;
        let protocols = wallet_utils::serde_func::serde_to_string(&input.protocols)?;

        let mut rec = sqlx::query_as::<_, ChainEntity>(sql)
            .bind(&input.name)
            .bind(&input.chain_code)
            .bind(&input.node_id)
            .bind(protocols)
            .bind(&input.main_symbol)
            .bind(input.status)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(rec.pop().ok_or(crate::DatabaseError::ReturningNone)?)
    }

    pub async fn list<'c, E>(executor: E) -> Result<Vec<ChainEntity>, crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = "SELECT * FROM chain;";

        sqlx::query_as::<sqlx::Sqlite, ChainEntity>(sql)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn one<'a, E>(executor: E, id: String) -> Result<Option<ChainWithNode>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        SELECT name, chain_code, node_id, protocols, main_symbol, status, created_at, updated_at
        FROM chain
        WHERE chain_code = $1;"#;
        sqlx::query_as::<sqlx::Sqlite, ChainWithNode>(sql)
            .bind(id)
            .fetch_optional(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list_with_node<'c, E>(executor: E) -> Result<Vec<ChainWithNode>, crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = "select q.*, a.rpc_url, a.ws_url, a.http_url, a.network, a.name as node_name
                        from chain as q  
                        join node a on q.node_id = a.node_id;";

        sqlx::query_as::<sqlx::Sqlite, ChainWithNode>(sql)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
