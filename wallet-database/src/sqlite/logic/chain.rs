use sqlx::{Executor, Sqlite};

use crate::entities::chain::ChainEntity;

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct StringList(pub Vec<String>);

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for StringList {
    fn decode(
        value: <sqlx::Sqlite as sqlx::database::HasValueRef<'r>>::ValueRef,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let value = <&str as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;

        // now you can parse this into your type (assuming there is a `FromStr`)
        // let value = value.as_str()?;
        let list: Vec<String> = serde_json::from_str(value)?;
        Ok(StringList(list))
    }
}

impl sqlx::Type<sqlx::Sqlite> for StringList {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

#[derive(Debug, Default, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ChainWithNode {
    pub name: String,
    pub chain_code: String,
    pub main_symbol: String,
    pub node_id: String,
    pub node_name: String,
    pub rpc_url: String,
    pub ws_url: String,
    pub http_url: String,
    pub network: String,
    pub status: u8,
    #[serde(skip_serializing)]
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    #[serde(skip_serializing)]
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

impl ChainEntity {
    pub async fn set_chain_node<'a, E>(
        executor: E,
        chain_code: &str,
        node_id: &str,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            update chain set 
                node_id = $2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            where chain_code = $1
            RETURNING *
            "#;

        sqlx::query_as::<sqlx::Sqlite, ChainEntity>(sql)
            .bind(chain_code)
            .bind(node_id)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn detail<'a, E>(exec: E, chain_code: &str) -> Result<Option<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
                        SELECT *
                        FROM chain
                        WHERE chain_code = $1;"#;
        sqlx::query_as::<sqlx::Sqlite, ChainEntity>(sql)
            .bind(chain_code)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn detail_by_id<'a, E>(exec: E, node_id: &str) -> Result<Option<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT *
            FROM chain
            WHERE node_id = $1;"#;
        sqlx::query_as::<sqlx::Sqlite, ChainEntity>(sql)
            .bind(node_id)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn detail_with_main_symbol<'a, E>(
        exec: E,
        main_symbol: &str,
    ) -> Result<Option<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
                        SELECT *
                        FROM chain
                        WHERE main_symbol = $1;"#;
        sqlx::query_as::<sqlx::Sqlite, ChainEntity>(sql)
            .bind(main_symbol)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list<'a, E>(exec: E) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM chain;";

        sqlx::query_as::<sqlx::Sqlite, ChainEntity>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list_with_node_info(
        db: std::sync::Arc<sqlx::Pool<sqlx::Sqlite>>,
    ) -> Result<Vec<ChainWithNode>, crate::Error> {
        let sql = "select q.*, a.rpc_url, a.ws_url, a.http_url, a.network, a.name as node_name
                            from chain as q  
                            join node a on q.node_id = a.node_id;";

        sqlx::query_as::<sqlx::Sqlite, ChainWithNode>(sql)
            .fetch_all(db.as_ref())
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn chain_node_info<'a, E>(
        exec: E,
        chain_code: &str,
    ) -> Result<Option<ChainWithNode>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
                            select q.*, a.rpc_url, a.ws_url, a.http_url, a.network, a.name as node_name
                            from chain as q  
                            join node a on q.node_id = a.node_id
                            where q.chain_code = ?"#;
        sqlx::query_as::<sqlx::Sqlite, ChainWithNode>(sql)
            .bind(chain_code)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_chain_list_in_chain_code(
        db: std::sync::Arc<sqlx::Pool<sqlx::Sqlite>>,
        chain_codes: Vec<&str>,
    ) -> Result<Vec<Self>, crate::Error> {
        let chain_codes = crate::sqlite::operator::any_in_collection(chain_codes, "','");
        let sql = format!(
            "SELECT name, chain_code, node_id, protocols, main_symbol, status, created_at, updated_at FROM chain
        WHERE chain_code in ('{}');",
            chain_codes
        );
        tracing::info!("sql: {sql}");
        sqlx::query_as::<sqlx::Sqlite, ChainEntity>(&sql)
            .bind(chain_codes)
            .fetch_all(db.as_ref())
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
