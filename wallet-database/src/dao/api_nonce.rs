use sqlx::{Executor, Sqlite};

pub(crate) struct ApiNonceDao;

impl ApiNonceDao {
    pub async fn get_api_nonce<'c, E>(
        executor: E,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<i64, crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"
            select nonce from api_nonce where from_addr = $1 and chain_code = $2
        "#;
        let nonce = sqlx::query_scalar::<_, i64>(sql)
            .bind(from_addr)
            .bind(chain_code)
            .fetch_one(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(nonce)
    }

    pub async fn get_api_nonce_optional<'c, E>(
        executor: E,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<Option<i64>, crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"
            select nonce from api_nonce where from_addr = $1 and chain_code = $2
        "#;
        let nonce = sqlx::query_scalar::<_, i64>(sql)
            .bind(from_addr)
            .bind(chain_code)
            .fetch_optional(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(nonce)
    }

    /// 单调追平 nonce：只允许将 api_nonce.nonce 提升到 floor_nonce（不会回滚）
    pub async fn upsert_nonce_floor<'c, E>(
        executor: E,
        from_addr: &str,
        chain_code: &str,
        floor_nonce: i64,
    ) -> Result<i64, crate::Error>
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
                nonce = case
                    when api_nonce.nonce is null then excluded.nonce
                    when api_nonce.nonce < excluded.nonce then excluded.nonce
                    else api_nonce.nonce
                end,
                updated_at = excluded.updated_at
            returning nonce
        "#;

        let nonce = sqlx::query_scalar::<_, i64>(sql)
            .bind(from_addr)
            .bind(chain_code)
            .bind(floor_nonce)
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
                nonce = coalesce(nonce, -1) + 1,
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

    /// 分页获取所有 nonce 记录，支持稳定排序和 cursor 分页
    pub async fn get_all_api_nonce_paginated<'c, E>(
        executor: E,
        cursor: Option<(&str, &str)>,
        limit: i32,
    ) -> Result<Vec<(String, String, i64)>, crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let (sql, params) = if let Some((last_addr, last_chain)) = cursor {
            // 使用 cursor 进行分页
            let sql = r#"
                select from_addr, chain_code, nonce
                from api_nonce
                where (from_addr > $1) or (from_addr = $1 and chain_code > $2)
                order by from_addr asc, chain_code asc
                limit $3
            "#;
            (sql, vec![last_addr.to_string(), last_chain.to_string(), limit.to_string()])
        } else {
            // 初始分页
            let sql = r#"
                select from_addr, chain_code, nonce
                from api_nonce
                order by from_addr asc, chain_code asc
                limit $1
            "#;
            (sql, vec![limit.to_string()])
        };

        let mut query = sqlx::query_as::<_, (String, String, i64)>(sql);
        for param in params {
            query = query.bind(param);
        }

        let results =
            query.fetch_all(executor).await.map_err(|e| crate::Error::Database(e.into()))?;

        Ok(results)
    }
}
