use sqlx::{Executor, Sqlite};

pub(crate) struct ApiNonceDao;

impl ApiNonceDao {
    pub async fn signed_nonce_decision(
        conn: &mut sqlx::SqliteConnection,
        address: &str,
        chain: &str,
        chain_next: i64,
    ) -> Result<crate::entities::api_nonce_claim::WithdrawNonceDecision, crate::Error> {
        let counter: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(nonce) FROM api_nonce WHERE lower(from_addr)=lower(?) AND chain_code=?",
        )
        .bind(address)
        .bind(chain)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        let claims = Self::nonce_claims(&mut *conn, address, chain, chain_next, i64::MAX).await?;
        // Explicit scoped recovery leaves the historical counter intact. Only
        // a verified consumed replacement authorizes ignoring that captured bound.
        let recovered: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(legacy_counter) FROM api_collect_replacement WHERE lower(from_addr)=lower(trim(?)) AND chain_code=? AND winner_hash IS NOT NULL AND block_hash IS NOT NULL AND nonce<?"
        ).bind(address).bind(chain).bind(chain_next).fetch_one(&mut *conn).await.map_err(|e|crate::Error::Database(e.into()))?;
        let counter = if recovered.is_some_and(|bound| counter.is_some_and(|n| n <= bound)) {
            None
        } else {
            counter
        };
        let last = claims
            .iter()
            .map(|c| c.nonce)
            .max()
            .unwrap_or(chain_next - 1)
            .max(counter.unwrap_or(chain_next - 1));
        Ok(crate::entities::api_nonce_claim::withdraw_nonce_decision(chain_next, last, &claims))
    }

    pub async fn nonce_claims<'c, E>(
        executor: E,
        address: &str,
        chain: &str,
        first: i64,
        last: i64,
    ) -> Result<Vec<crate::entities::api_nonce_claim::NonceClaim>, crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"
          SELECT CASE WHEN trade_type=1 THEN 'withdraw' ELSE 'ordinary' END AS kind,
            nonce,raw_tx,tx_hash,err_code,err_msg,failure_stage,last_broadcast_at,
            (broadcast_uncertain_since_at IS NOT NULL OR broadcast_uncertain_retry_count>0
             OR broadcast_uncertain_rebroadcast_count>0 OR broadcast_uncertain_last_checked_at IS NOT NULL OR broadcast_uncertain_reconciled_at IS NOT NULL) AS uncertain,
            (transaction_time IS NOT NULL OR chain_success_at IS NOT NULL OR chain_failed_at IS NOT NULL) AS executed,
            (err_code IS NOT NULL OR finished_at IS NOT NULL) AS terminal
          FROM api_withdraws WHERE lower(from_addr)=lower($1) AND chain_code=$2 AND nonce BETWEEN $3 AND $4
            AND (raw_tx IS NOT NULL OR tx_hash IS NOT NULL OR last_broadcast_at IS NOT NULL OR transaction_time IS NOT NULL
             OR broadcast_uncertain_since_at IS NOT NULL OR broadcast_uncertain_retry_count>0
             OR broadcast_uncertain_rebroadcast_count>0 OR broadcast_uncertain_last_checked_at IS NOT NULL)
          UNION ALL
          SELECT 'collect',nonce,raw_tx,tx_hash,err_code,err_msg,NULL,last_broadcast_at,
            (broadcast_uncertain_since_at IS NOT NULL OR broadcast_uncertain_retry_count>0
             OR broadcast_uncertain_rebroadcast_count>0 OR broadcast_uncertain_last_checked_at IS NOT NULL),
            transaction_time IS NOT NULL,(err_code IS NOT NULL OR finished_at IS NOT NULL)
          FROM api_collect WHERE lower(from_addr)=lower($1) AND chain_code=$2 AND nonce BETWEEN $3 AND $4
            AND (raw_tx IS NOT NULL OR tx_hash IS NOT NULL OR last_broadcast_at IS NOT NULL OR transaction_time IS NOT NULL
             OR broadcast_uncertain_since_at IS NOT NULL OR broadcast_uncertain_retry_count>0
             OR broadcast_uncertain_rebroadcast_count>0 OR broadcast_uncertain_last_checked_at IS NOT NULL)
          UNION ALL
          SELECT 'fee',nonce,raw_tx,tx_hash,err_code,err_msg,NULL,last_broadcast_at,
            (broadcast_uncertain_since_at IS NOT NULL OR broadcast_uncertain_retry_count>0
             OR broadcast_uncertain_rebroadcast_count>0 OR broadcast_uncertain_last_checked_at IS NOT NULL),
            transaction_time IS NOT NULL,(err_code IS NOT NULL OR finished_at IS NOT NULL)
          FROM api_fee WHERE lower(from_addr)=lower($1) AND chain_code=$2 AND nonce BETWEEN $3 AND $4
            AND (raw_tx IS NOT NULL OR tx_hash IS NOT NULL OR last_broadcast_at IS NOT NULL OR transaction_time IS NOT NULL
             OR broadcast_uncertain_since_at IS NOT NULL OR broadcast_uncertain_retry_count>0
             OR broadcast_uncertain_rebroadcast_count>0 OR broadcast_uncertain_last_checked_at IS NOT NULL)
        "#;
        sqlx::query_as(sql)
            .bind(address)
            .bind(chain)
            .bind(first)
            .bind(last)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

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

    /// 精确设置 nonce：将 api_nonce.nonce 覆盖为 exact_nonce（允许前移或回退）
    ///
    /// 用途：
    /// - 仅供 EVM uncertain timeout + nonce gap 场景下的强制对齐使用
    /// - 不做单调保护，调用方需保证使用场景安全
    pub async fn upsert_nonce_exact<'c, E>(
        executor: E,
        from_addr: &str,
        chain_code: &str,
        exact_nonce: i64,
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
                nonce = excluded.nonce,
                updated_at = excluded.updated_at
            returning nonce
        "#;

        let nonce = sqlx::query_scalar::<_, i64>(sql)
            .bind(from_addr)
            .bind(chain_code)
            .bind(exact_nonce)
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
