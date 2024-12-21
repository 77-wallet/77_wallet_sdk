use sqlx::{Executor, Sqlite};

use crate::entities::node::{NodeCreateVo, NodeEntity};

impl NodeCreateVo {
    pub fn new(
        node_id: &str,
        name: &str,
        chain_code: &str,
        rpc_url: &str,
        http_url: Option<String>,
    ) -> NodeCreateVo {
        Self {
            node_id: node_id.to_string(),
            name: name.to_string(),
            chain_code: chain_code.to_string(),
            rpc_url: rpc_url.to_string(),
            ws_url: "".to_string(),
            http_url: http_url.unwrap_or_default(),
            network: "mainnet".to_string(),
            status: 1,
            is_local: 0,
        }
    }

    pub fn with_status(mut self, status: u8) -> Self {
        self.status = status;
        self
    }

    pub fn with_http_url(mut self, http_url: &str) -> Self {
        self.http_url = http_url.to_string();
        self
    }

    pub fn with_network(mut self, network: &str) -> Self {
        self.network = network.to_string();
        self
    }

    pub fn with_is_local(mut self, is_local: u8) -> Self {
        self.is_local = is_local;
        self
    }
}

impl NodeEntity {
    pub async fn upsert<'a, E>(exec: E, req: NodeCreateVo) -> Result<NodeEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"Insert into node 
                (node_id, name, chain_code, status, is_local, 
                    rpc_url, ws_url, http_url, network, 
                    created_at, updated_at)
                values ($1, $2, $3, $4, $5, 
                        $6, $7, $8, $9,
                        strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                on conflict (node_id)
                do update set
                    rpc_url = excluded.rpc_url,
                    ws_url = excluded.ws_url,
                    http_url = excluded.http_url,
                    name = excluded.name,
                    status =  excluded.status,
                    network = excluded.network,
                    updated_at = excluded.updated_at
                returning *"#;

        // let params = vec![req.name.as_str(), req.chain_code.as_str()];
        // let node_id = wallet_utils::snowflake::gen_hash_uid(params);

        let mut rec = sqlx::query_as::<_, NodeEntity>(sql)
            .bind(&req.node_id)
            .bind(&req.name)
            .bind(&req.chain_code)
            .bind(req.status)
            .bind(req.is_local)
            .bind(&req.rpc_url)
            .bind(&req.ws_url)
            .bind(&req.http_url)
            .bind(&req.network)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(rec.pop().ok_or(crate::DatabaseError::ReturningNone)?)
    }

    // pub async fn detail(
    //     db: std::sync::Arc<sqlx::Pool<sqlx::Sqlite>>,
    //     id: i64,
    // ) -> Result<Option<Self>, crate::Error> {
    //     let sql = r#"
    //                     SELECT *
    //                     FROM node
    //                     WHERE id = $1;"#;
    //     sqlx::query_as::<sqlx::Sqlite, NodeEntity>(sql)
    //         .bind(id)
    //         .fetch_optional(db.as_ref())
    //         .await
    //         .map_err(|e| crate::Error::Database(e.into()))
    // }

    pub async fn list<'a, E>(
        exec: E,
        chain_codes: Vec<&str>,
        is_local: Option<u8>,
        status: Option<u8>,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = "SELECT * FROM node".to_string();
        let chain_codes = crate::any_in_collection(chain_codes, "','");
        let mut conditions = Vec::new();

        if !chain_codes.is_empty() {
            let str = format!("chain_code in ('{}')", chain_codes);
            conditions.push(str)
        }

        if is_local.is_some() {
            conditions.push("is_local = ?".to_string());
        }
        if status.is_some() {
            conditions.push("status = ?".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        let mut query = sqlx::query_as::<_, Self>(&sql);

        if let Some(is_local) = is_local {
            query = query.bind(is_local);
        }
        if let Some(status) = status {
            query = query.bind(status);
        }
        query
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    // pub async fn get_node_list_in_chain_codes<'a, E>(
    //     exec: E,
    //     chain_codes: Vec<&str>,
    //     status: Option<u8>,
    // ) -> Result<Vec<Self>, crate::Error>
    // where
    //     E: Executor<'a, Database = Sqlite>,
    // {
    //     let chain_codes = crate::any_in_collection(chain_codes, "','");
    //     let sql = format!(
    //         "SELECT * FROM node
    //     WHERE chain_code in ('{}');",
    //         chain_codes
    //     );
    //     sqlx::query_as::<sqlx::Sqlite, NodeEntity>(&sql)
    //         .bind(chain_codes)
    //         .fetch_all(exec)
    //         .await
    //         .map_err(|e| crate::Error::Database(e.into()))
    // }

    pub async fn delete<'a, E>(
        exec: E,
        node_id: &str,
        // rpc_url: &str,
        // chain_code: &str,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            DELETE FROM node
            WHERE node_id = $1
            RETURNING *
            "#;

        sqlx::query_as::<sqlx::Sqlite, Self>(sql)
            // .bind(rpc_url)
            // .bind(chain_code)
            .bind(node_id)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
