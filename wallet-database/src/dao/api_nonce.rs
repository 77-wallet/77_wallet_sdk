use sqlx::{Executor, Sqlite};

pub(crate) struct ApiNonceDao;

impl ApiNonceDao {
    pub async fn get_api_nonce<'c, E>(
        executor: E,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<i32, crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"
            select nonce from api_nonce where from_addr = $1 and chain_code = $2
        "#;
        let nonce = sqlx::query_scalar::<_, i32>(sql)
            .bind(from_addr)
            .bind(chain_code)
            .fetch_one(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(nonce)
    }

    pub async fn upsert_and_get_api_nonce<'c, E>(
        executor: E,
        from_addr: &str,
        chain_code: &str,
        nonce: i32,
    ) -> Result<i32, crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"
            Insert into api_nonce
                (from_addr,chain_code,nonce,created_at,updated_at)
            values
                ($1, $2, $3, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            on conflict (from_addr,chain_code)
            do update set
                nonce = nonce + 1,
                updated_at = excluded.updated_at
            returning nonce
        "#;

        let nonce = sqlx::query_scalar::<_, i32>(sql)
            .bind(from_addr)
            .bind(chain_code)
            .bind(nonce)
            .fetch_one(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(nonce)
    }
}
