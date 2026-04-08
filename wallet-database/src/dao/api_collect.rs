use crate::{
    DbPool,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus, CollectCreatedFact, ErrCode},
    pagination::Pagination,
};
use sqlx::{Executor, Row, Sqlite};

// ⚠️ finished_at 为链终态事实字段
// ⚠️ 除 mark_chain_finished / mark_result_ack_confirmed_and_chain_finished 外，禁止任何 UPDATE 语句写入 finished_at
// ⚠️ 未来 code review 时，搜索 `finished_at =` 并拒绝除上述方法外的所有情况

pub(crate) struct ApiCollectDao;

#[allow(dead_code)]
impl ApiCollectDao {
    pub async fn all_api_collect<'a, E>(exec: E) -> Result<Vec<ApiCollectEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"SELECT * FROM api_collect"#;
        let result = sqlx::query_as::<_, ApiCollectEntity>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    pub async fn page_api_collect<'a, E>(
        exec: &E,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiCollectEntity>, crate::Error>
    where
        for<'c> &'c E: Executor<'c, Database = Sqlite>,
    {
        let mut sql = "SELECT * FROM api_collect".to_string();
        sql.push_str(" ORDER BY updated_at DESC, created_at DESC");
        let paginate = Pagination::<ApiCollectEntity>::init(page, page_size);
        Ok(paginate.page(exec, &sql).await?)
    }

    pub async fn page_api_collect_with_status<'a, E>(
        exec: E,
        _page: i64,
        page_size: i64,
        vec_status: &[ApiCollectStatus],
    ) -> Result<(i64, Vec<ApiCollectEntity>), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Clone,
    {
        let placeholders = vec_status.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let count_sql =
            format!("SELECT count(*) FROM api_collect where status in ({})", placeholders);
        let sql = format!(
            "SELECT * FROM api_collect where status in ({}) ORDER BY id ASC LIMIT ?",
            placeholders
        );

        let mut query = sqlx::query_scalar::<_, i64>(&count_sql);
        for status in vec_status {
            query = query.bind(status);
        }
        let count =
            query.fetch_one(exec.clone()).await.map_err(|e| crate::Error::Database(e.into()))?;

        let mut query = sqlx::query_as::<_, ApiCollectEntity>(&sql);
        for status in vec_status {
            query = query.bind(status);
        }
        let res = query
            .bind(page_size)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok((count, res))
    }

    pub async fn get_api_collect_by_trade_no<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<ApiCollectEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_collect WHERE trade_no = ?";
        let res = sqlx::query_as::<_, ApiCollectEntity>(sql)
            .bind(trade_no)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    pub async fn get_api_collect_by_trade_no_status<'a, E>(
        exec: E,
        trade_no: &str,
        vec_status: &[ApiCollectStatus],
    ) -> Result<ApiCollectEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let placeholders = vec_status.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT * FROM api_collect where trade_no = ? AND status in ({})",
            placeholders
        );
        let mut query = sqlx::query_as::<_, ApiCollectEntity>(&sql).bind(trade_no);
        for status in vec_status {
            query = query.bind(status);
        }
        let res = query.fetch_one(exec).await.map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    /// Find collect candidates for runtime repair from acct_change facts.
    ///
    /// Notes:
    /// - Does NOT depend on local tx_hash being present (it may be missing/corrupted)
    /// - Does NOT require transaction_time to exist (MQTT acct_change may arrive before it)
    /// - Requires execution evidence (broadcast or chain time) before hash backfill is attempted
    /// - Returns only not-finished / not-error records that still need tx_exec_receipt hash backfill
    pub async fn find_candidates_for_acct_change_repair<'a, E>(
        exec: E,
        chain_code: &str,
        from_addr: &str,
        to_addr: &str,
        token_addr: Option<&str>,
        symbol: &str,
        limit: i64,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let base_sql = r#"
            SELECT * FROM api_collect
            WHERE chain_code = ?
              AND from_addr = ?
              AND to_addr = ?
              AND symbol = ?
              AND finished_at IS NULL
              AND err_code IS NULL
              AND tx_exec_receipt_uploaded_at IS NULL
              AND (last_broadcast_at IS NOT NULL OR transaction_time IS NOT NULL)
              AND (tx_hash IS NULL OR trim(tx_hash) = '')
        "#;
        // Token matching follows acct_change normalization:
        // - None => native coin rows (NULL / empty token_addr)
        // - Some(token) => exact token_addr match

        let (sql, token_to_bind) = match token_addr {
            Some(token) => (
                format!(
                    "{} AND token_addr = ? ORDER BY COALESCE(transaction_time, last_broadcast_at, updated_at, created_at) DESC LIMIT ?",
                    base_sql
                ),
                Some(token),
            ),
            None => (
                format!(
                    "{} AND (token_addr IS NULL OR trim(token_addr) = '') ORDER BY COALESCE(transaction_time, last_broadcast_at, updated_at, created_at) DESC LIMIT ?",
                    base_sql
                ),
                None,
            ),
        };

        let mut query = sqlx::query_as::<_, ApiCollectEntity>(&sql)
            .bind(chain_code)
            .bind(from_addr)
            .bind(to_addr)
            .bind(symbol);

        if let Some(token) = token_to_bind {
            query = query.bind(token);
        }

        // Keep this query intentionally broad; caller performs amount/time-window/uniqueness checks.
        let result = query
            .bind(limit)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    async fn upsert<'c, E>(executor: E, input: ApiCollectEntity) -> Result<(), crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"
            Insert into api_collect
                (id,uid,name,from_addr,to_addr,value,chain_code,token_addr,symbol,trade_no,trade_type,status,created_at,updated_at)
            values
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            on conflict (trade_no)
            do update set
                risk_addr = excluded.risk_addr,
                status = excluded.status,
                updated_at = excluded.updated_at
            returning *
        "#;

        let _ = sqlx::query_as::<_, ApiCollectEntity>(sql)
            .bind(&input.uid)
            .bind(&input.name)
            .bind(&input.from_addr)
            .bind(&input.to_addr)
            .bind(&input.value)
            .bind(&input.chain_code)
            .bind(&input.token_addr)
            .bind(&input.symbol)
            .bind(&input.trade_no)
            .bind(&input.risk_addr)
            .bind(&input.trade_type)
            .bind(&input.status)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    /// 接单事实写入（Order ACK）
    ///
    /// ⚠️ 只允许写入"接单阶段必然存在"的字段
    /// ⚠️ 严禁写入任何链上执行相关字段
    /// ⚠️ 本方法不参与状态推进
    ///
    /// ✅ 设计原则：
    /// - INSERT 只写"接单阶段必然存在、且没有 DEFAULT 的字段"
    /// - 其余字段一律交给 DEFAULT / NULL
    pub async fn add<'a, E>(exec: E, api_collect: CollectCreatedFact) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO api_collect
                (uid,
                name,
                from_addr,
                to_addr,
                value,
                validate,
                chain_code,
                token_addr,
                symbol,
                trade_no,
                trade_type,
                risk_addr,
                status,
                ever_needed_service_fee,
                created_at)
            VALUES
                ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,false,
                strftime('%Y-%m-%dT%H:%M:%SZ','now'))
            ON CONFLICT(trade_no) DO UPDATE SET
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
        "#;

        let res = sqlx::query(sql)
            .bind(&api_collect.uid)
            .bind(&api_collect.name)
            .bind(&api_collect.from_addr)
            .bind(&api_collect.to_addr)
            .bind(&api_collect.value)
            .bind(&api_collect.validate)
            .bind(&api_collect.chain_code)
            .bind(&api_collect.token_addr)
            .bind(&api_collect.symbol)
            .bind(&api_collect.trade_no)
            .bind(&api_collect.trade_type)
            .bind(&api_collect.risk_addr)
            .bind(&api_collect.status)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        tracing::info!(rows_affected=%res.rows_affected(), trade_no=%api_collect.trade_no, "collect api add success");
        Ok(())
    }

    pub async fn update_to_addr<'a, E>(
        exec: E,
        trade_no: &str,
        to_addr: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                to_addr = $2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(to_addr)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        tracing::info!(xx=%res.rows_affected(), "collect api update to_addr");
        Ok(())
    }

    /// 更新交易状态和nonce
    ///
    /// ⚠️ Legacy: 半事实聚合写入
    /// ⚠️ 复合写多个字段，未来可能出现部分写入问题
    /// ⚠️ 建议使用原子事实跃迁方法替代
    ///
    /// ⚠️ 写入顺序约束（不可逆）：
    /// raw_tx → tx_hash → transaction_time → finished_at
    /// - 不允许写tx_hash时raw_tx还是NULL
    /// - 不允许写transaction_time时tx_hash是NULL
    /// - 不允许写finished_at时transaction_time是NULL
    pub async fn update_tx_status_nonce(
        pool: &DbPool,
        from_addr: &str,
        chain_code: &str,
        trade_no: &str,
        nonce: i64,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiCollectStatus,
    ) -> Result<u64, crate::Error> {
        crate::db::sqlite_retry::with_sqlite_locked_retry(|| async {
            let mut tx = pool.begin().await.map_err(|e| crate::Error::Database(e.into()))?;
            let sql = r#"
            UPDATE api_collect
            SET
                tx_hash = $2,
                nonce = $3,
                resource_consume = $4,
                transaction_fee = $5,
                status = $6,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND raw_tx IS NOT NULL
        "#;

            let res = sqlx::query(sql)
                .bind(trade_no)
                .bind(tx_hash)
                .bind(nonce)
                .bind(resource_consume)
                .bind(transaction_fee)
                .bind(&status)
                .execute(&mut *tx)
                .await
                .map_err(|e| crate::Error::Database(e.into()))?;

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

            sqlx::query_scalar::<_, i32>(sql)
                .bind(from_addr)
                .bind(chain_code)
                .bind(nonce)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| crate::Error::Database(e.into()))?;

            tx.commit().await.map_err(|e| crate::Error::Database(e.into()))?;

            Ok(res.rows_affected())
        })
        .await
    }

    /// 更新交易状态
    ///
    /// ⚠️ 写入顺序约束（不可逆）：
    /// raw_tx → tx_hash → transaction_time → finished_at
    /// - 不允许写tx_hash时raw_tx还是NULL
    /// - 不允许写transaction_time时tx_hash是NULL
    /// - 不允许写finished_at时transaction_time是NULL
    pub async fn update_tx_status<'a, E>(
        exec: E,
        trade_no: &str,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiCollectStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                tx_hash = $2,
                resource_consume = $3,
                transaction_fee = $4,
                status = $5,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND raw_tx IS NOT NULL
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(tx_hash)
            .bind(resource_consume)
            .bind(transaction_fee)
            .bind(&status)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    pub async fn update_status_and_err<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiCollectStatus,
        err_code: ErrCode,
        err_msg: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                status = $2,
                err_code = $3,
                err_msg = $4,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(&status)
            .bind(err_code)
            .bind(err_msg)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 原子确认交易成功（事实跃迁）
    ///
    /// 语义：
    /// - 这是"广播成功 → 链上确认"的不可逆事实跃迁
    /// - 单条 SQL 原子更新，防止 kill -9 产生"半完成事实"
    /// - WHERE 带旧事实约束，保证并发安全
    ///
    /// 写入顺序约束（不可逆）：
    /// raw_tx → tx_hash → transaction_time → finished_at
    #[allow(deprecated)]
    #[deprecated(note = "LEGACY STATE MACHINE API. \
                Do NOT use in Shadow / Scanner / fact-driven paths. \
                Use fact-based APIs instead.")]
    pub async fn legacy_confirm_transaction<'a, E>(
        exec: E,
        trade_no: &str,
        tx_hash: &str,
        transaction_time: &str,
        transaction_fee: &str,
        resource_consume: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                tx_hash = $2,
                transaction_time = $3,
                transaction_fee = $4,
                resource_consume = $5,
                err_code = NULL,
                err_msg = NULL,
                broadcast_uncertain_since_at = NULL,
                broadcast_uncertain_retry_count = 0,
                broadcast_uncertain_last_checked_at = NULL,
                broadcast_uncertain_reconciled_at = NULL,
                broadcast_uncertain_rebroadcast_count = 0,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND transaction_time IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(tx_hash)
            .bind(transaction_time)
            .bind(transaction_fee)
            .bind(resource_consume)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 向后兼容包装器
    #[allow(deprecated)]
    #[deprecated(note = "Compatibility wrapper. \
                New code MUST NOT use this API.")]
    pub async fn confirm_transaction<'a, E>(
        exec: E,
        trade_no: &str,
        tx_hash: &str,
        transaction_time: &str,
        transaction_fee: &str,
        resource_consume: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        Self::legacy_confirm_transaction(
            exec,
            trade_no,
            tx_hash,
            transaction_time,
            transaction_fee,
            resource_consume,
        )
        .await
    }

    /// Confirm on-chain transaction finality (fact-based)
    ///
    /// Semantics:
    /// - On-chain transaction has been proven finalized
    /// - This is a fact write, NOT a state-machine transition
    /// - Idempotent
    ///
    /// Does NOT:
    /// - imply broadcast success
    /// - write finished_at
    /// - trigger side effects
    ///
    /// Who can write transaction_time:
    /// | Scenario             | Write transaction_time | Who writes         |
    /// | -------------------- | --------------------- | ------------------ |
    /// | Broadcast success    | ❌                     | No one             |
    /// | MQTT TxRes           | ❌                     | No one             |
    /// | Scanner chain check  | ✅                     | Scanner / Shadow   |
    /// | Recovery chain check | ✅                     | Shadow             |
    pub async fn confirm_onchain_transaction_fact<'a, E>(
        exec: E,
        trade_no: &str,
        tx_hash: &str,
        transaction_time: &str,
        transaction_fee: &str,
        resource_consume: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                tx_hash = $2,
                transaction_time = $3,
                transaction_fee = $4,
                resource_consume = $5,
                broadcast_uncertain_since_at = NULL,
                broadcast_uncertain_retry_count = 0,
                broadcast_uncertain_last_checked_at = NULL,
                broadcast_uncertain_reconciled_at = NULL,
                broadcast_uncertain_rebroadcast_count = 0,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND transaction_time IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(tx_hash)
            .bind(transaction_time)
            .bind(transaction_fee)
            .bind(resource_consume)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// Confirm on-chain transaction finality with recover (fact-based)
    ///
    /// Semantics (RECOVER FACT COMPLETION):
    /// - On-chain transaction has been proven finalized via recover
    /// - This implies broadcast MUST have happened (behavior fact)
    /// - Atomically completes both behavior and chain facts
    /// - Idempotent
    ///
    /// Fact completion guarantee:
    /// - If transaction_time is set, last_broadcast_at MUST also be set
    /// - Uses COALESCE to preserve existing broadcast timestamps
    /// - Only updates when transaction_time IS NULL (幂等)
    ///
    /// Time source guarantee:
    /// - transaction_time MUST come from on-chain confirmation (chain timestamp)
    /// - last_broadcast_at is backfilled with the same value as transaction_time
    /// - This ensures both fields reflect the same chain-based timestamp
    ///
    /// Who can call this:
    /// | Scenario             | Can call | Reason               |
    /// | -------------------- | -------- | -------------------- |
    /// | Recovery chain check | ✅        | Recover fact completion |
    /// | Scanner chain check  | ❌        | Use regular confirm  |
    /// | Broadcast success    | ❌        | Use mark_broadcast_executed |
    pub async fn confirm_onchain_transaction_fact_with_recover<'a, E>(
        exec: E,
        trade_no: &str,
        tx_hash: &str,
        last_broadcast_at: &str,
        transaction_time: &str,
        transaction_fee: &str,
        resource_consume: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                last_broadcast_at = COALESCE(last_broadcast_at, $3),
                tx_hash = $2,
                transaction_time = $4,
                transaction_fee = $5,
                resource_consume = $6,
                err_code = NULL,
                err_msg = NULL,
                broadcast_uncertain_since_at = NULL,
                broadcast_uncertain_retry_count = 0,
                broadcast_uncertain_last_checked_at = NULL,
                broadcast_uncertain_reconciled_at = NULL,
                broadcast_uncertain_rebroadcast_count = 0,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND transaction_time IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(tx_hash)
            .bind(last_broadcast_at)
            .bind(transaction_time)
            .bind(transaction_fee)
            .bind(resource_consume)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 确认交易时间（如果不存在）
    ///
    /// 语义：
    /// - 只写入 transaction_time 字段
    /// - 仅当 transaction_time IS NULL 时才写入
    /// - 幂等
    pub async fn confirm_transaction_time_if_absent<'a, E>(
        exec: E,
        trade_no: &str,
        transaction_time: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                transaction_time = $2,
                err_code = NULL,
                err_msg = NULL,
                broadcast_uncertain_since_at = NULL,
                broadcast_uncertain_retry_count = 0,
                broadcast_uncertain_last_checked_at = NULL,
                broadcast_uncertain_reconciled_at = NULL,
                broadcast_uncertain_rebroadcast_count = 0,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND transaction_time IS NULL
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(transaction_time)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// Backfill tx_hash only when it is currently missing and execution has progressed.
    ///
    /// Safety rules:
    /// - Only writes when tx_hash is NULL/empty
    /// - Requires strong execution evidence: transaction_time OR last_broadcast_at exists
    /// - Never overwrites a non-empty tx_hash
    pub async fn backfill_tx_hash_if_missing<'a, E>(
        exec: E,
        trade_no: &str,
        tx_hash: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                tx_hash = $2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND (tx_hash IS NULL OR trim(tx_hash) = '')
              AND (transaction_time IS NOT NULL OR last_broadcast_at IS NOT NULL)
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(tx_hash)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// ⚠️ Legacy: 状态机时代的遗留方法，使用status作为决策条件
    /// ⚠️ 未来应该移除，改用事实驱动的状态更新
    /// ⚠️ 禁止Scanner/Executor使用此方法
    #[allow(deprecated)]
    #[deprecated(note = "LEGACY STATE MACHINE API. \
                Do NOT use in Shadow / Scanner / fact-driven paths. \
                Use fact-based APIs instead.")]
    pub async fn legacy_update_next_status<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiCollectStatus,
        next_status: ApiCollectStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                status = $3,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1 and status = $2
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(&status)
            .bind(&next_status)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 向后兼容包装器
    #[allow(deprecated)]
    #[deprecated(note = "Compatibility wrapper. \
                New code MUST NOT use this API.")]
    pub async fn update_next_status<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiCollectStatus,
        next_status: ApiCollectStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        Self::legacy_update_next_status(exec, trade_no, status, next_status).await
    }

    /// ⚠️ Legacy: 状态机时代的遗留方法，使用status作为决策条件
    /// ⚠️ 未来应该移除，改用事实驱动的状态更新
    /// ⚠️ 禁止Scanner/Executor使用此方法
    #[allow(deprecated)]
    #[deprecated(note = "LEGACY STATE MACHINE API. \
                Do NOT use in Shadow / Scanner / fact-driven paths. \
                Use fact-based APIs instead.")]
    pub async fn legacy_update_next_status_and_err<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiCollectStatus,
        next_status: ApiCollectStatus,
        err_code: u32,
        err_msg: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                status = $3,
                err_code = $4,
                err_msg = $5,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1 and status = $2
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(&status)
            .bind(&next_status)
            .bind(err_code)
            .bind(err_msg)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 向后兼容包装器
    #[allow(deprecated)]
    #[deprecated(note = "Compatibility wrapper. \
                New code MUST NOT use this API.")]
    pub async fn update_next_status_and_err<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiCollectStatus,
        next_status: ApiCollectStatus,
        err_code: u32,
        err_msg: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        Self::legacy_update_next_status_and_err(
            exec,
            trade_no,
            status,
            next_status,
            err_code,
            err_msg,
        )
        .await
    }

    pub async fn update_post_tx_count<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                post_tx_count = MIN(post_tx_count + 1, 63),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1 
              AND transaction_time IS NULL
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    pub async fn update_post_confirm_tx_count<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                post_confirm_tx_count = MIN(post_confirm_tx_count + 1, 63),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1 
              AND transaction_time IS NOT NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 更新构建完成后的交易信息，包括raw_tx、tx_hash、transaction_fee和building_at
    ///
    /// ⚠️ 写入顺序约束（不可逆）：
    /// raw_tx → tx_hash → transaction_time → finished_at
    /// - 不允许写tx_hash时raw_tx还是NULL
    /// - 不允许写transaction_time时tx_hash是NULL
    /// - 不允许写finished_at时transaction_time是NULL
    pub async fn update_after_build<'a, E>(
        exec: E,
        trade_no: &str,
        tx_hash: &str,
        raw_tx: &str,
        transaction_fee: &str,
        nonce: i64,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                raw_tx = $3,
                tx_hash = $2,
                transaction_fee = $4,
                nonce = $5,
                building_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND raw_tx IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(tx_hash)
            .bind(raw_tx)
            .bind(transaction_fee)
            .bind(nonce)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 兼容保留：标记订单 ACK 尝试
    ///
    /// 当前 attempted_at 字段已移除，此方法仅刷新 updated_at 以保持调用兼容。
    pub async fn mark_order_ack_attempted<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND order_ack_sent_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 标记订单 ACK 已发送（推进事实）
    ///
    /// 语义：
    /// - 订单 ACK 已成功发送到后端
    /// - 这是副作用完成的事实
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在订单 ACK 已尝试的前提下调用
    /// - 仅允许调用一次（order_ack_sent_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_order_ack_sent<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                order_ack_sent_at = COALESCE(
                    order_ack_sent_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND order_ack_sent_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    pub async fn get_ack_times<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<
        (
            Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
            Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        ),
        crate::Error,
    >
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT order_ack_sent_at, result_ack_sent_at
            FROM api_collect
            WHERE trade_no = $1
        "#;
        let row = sqlx::query(sql)
            .bind(trade_no)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        if let Some(row) = row {
            let order_ack_sent_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>> =
                row.try_get(0).map_err(|e| crate::Error::Database(e.into()))?;
            let result_ack_sent_at: Option<
                sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
            > = row.try_get(1).map_err(|e| crate::Error::Database(e.into()))?;

            Ok((order_ack_sent_at, result_ack_sent_at))
        } else {
            Ok((None, None))
        }
    }

    /// 扫描可构建的交易：raw_tx为空且未进入终态
    ///
    /// ⚠️ 核心事实驱动原则：
    /// - 只基于不可逆事实字段(raw_tx)决策
    /// - building_at 仅作为 worker 入口的建单占位，不参与 scanner 决策
    /// - 并发通过 worker 入口原子 claim + raw_tx 写入唯一性保证
    ///
    /// ⚠️ 强顺序屏障：
    /// - BuildTx 必须发生在 OrderAck 之后
    /// - 禁止移除 order_ack_sent_at 条件，否则会破坏强顺序保证
    ///
    /// ⚠️ 与 TxFeeResAck 的关系：
    /// - 如果曾经缺过手续费，则必须先完成 TxFeeResAck，才能重新构建
    /// - TxFeeResAck 仍然是 broadcast 的前置条件
    pub async fn scan_can_build<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            -- ⚠️ 强顺序屏障：
            -- BuildTx 必须发生在 OrderAck 之后
            -- 禁止移除 order_ack_sent_at 条件，否则会破坏强顺序保证
            -- 如果曾经缺过手续费，则必须先完成 TxFeeResAck
            SELECT * FROM api_collect 
            WHERE order_ack_sent_at IS NOT NULL
            AND raw_tx IS NULL 
            AND (need_service_fee IS NULL OR need_service_fee = false)
            AND (ever_needed_service_fee = false OR tx_fee_res_ack_sent_at IS NOT NULL)
            AND transaction_time IS NULL
            AND finished_at IS NULL
            AND err_code IS NULL
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiCollectEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 扫描可广播的交易：raw_tx存在且transaction_time为空
    ///
    /// ⚠️ 核心事实驱动原则：
    /// - 只基于不可逆事实字段(raw_tx, transaction_time)决策
    /// - 不依赖时间字段(last_broadcast_at)进行决策
    /// - 并发通过transaction_time写入唯一性保证
    pub async fn scan_can_broadcast<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_collect 
            WHERE raw_tx IS NOT NULL 
            AND last_broadcast_at IS NULL 
            AND finished_at IS NULL 
            AND err_code IS NULL 
            AND order_ack_sent_at IS NOT NULL
            AND (ever_needed_service_fee = false OR tx_fee_res_ack_sent_at IS NOT NULL)
            AND (chain_code NOT IN ('bnb','eth') OR broadcast_uncertain_since_at IS NULL)
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiCollectEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 扫描已确认且需要发送 Result ACK 的交易
    ///
    /// 事实条件直接翻译：
    /// - tx_exec_receipt_uploaded_at IS NOT NULL：交易执行回执已上传
    /// - finished_at IS NULL：系统生命周期未结束
    /// - result_ack_sent_at IS NULL：尚未发送结果确认（推进事实）
    ///
    /// ⚠️ 强顺序屏障：
    /// - ResultAck 必须发生在 TxExecReceipt 上传之后
    /// - 禁止使用 transaction_time 作为前置条件（共享前提事实）
    ///
    /// ⚠️ 注意：
    /// - 不检查尝试中间态（attempted 语义不参与 Scanner 判断）
    /// - attempted 只用于 Worker / 运维观测
    pub async fn scan_confirmed_need_result_ack<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_collect 
            WHERE transaction_time IS NOT NULL
            AND tx_res_received_at IS NOT NULL
            AND finished_at IS NULL
            AND result_ack_sent_at IS NULL
            AND err_code IS NULL
            ORDER BY transaction_time ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiCollectEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 扫描已确认但未上传服务费的交易
    pub async fn scan_confirmed_need_service_fee_upload<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_collect 
            WHERE need_service_fee = true
            AND service_fee_uploaded_at IS NULL
            AND err_code IS NULL
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiCollectEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 扫描需要发送手续费结果确认 ACK 的交易
    pub async fn scan_confirmed_need_tx_fee_res_ack<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_collect 
            WHERE (need_service_fee IS NULL OR need_service_fee = false)
            AND ever_needed_service_fee = true
            AND tx_fee_res_ack_sent_at IS NULL
            AND last_broadcast_at IS NULL
            AND transaction_time IS NULL
            AND finished_at IS NULL
            AND err_code IS NULL
            ORDER BY updated_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiCollectEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 更新 building_at 时间。
    ///
    /// `building_at` 只作为短期 build-slot 占位，用于避免同一 trade_no
    /// 在构建期间被重复推进；不是最终事实字段。
    pub async fn update_building_at<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                building_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND building_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 清除 building_at 占位。
    ///
    /// 用于 BuildTx 失败后的释放，避免占位残留导致后续扫描反复命中但又无法 claim。
    pub async fn clear_building_at<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                building_at = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND building_at IS NOT NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 更新 last_broadcast_at 时间。
    pub async fn update_last_broadcast_at<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND (last_broadcast_at IS NULL OR last_broadcast_at < datetime('now', '-1 minute'))
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 作废当前 raw_tx 及其 tx_hash
    ///
    /// ⚠️ 设计铁律：
    /// - 一旦 raw_tx 被判定为不可再广播 / 不可再构建（如手续费不足、前置条件变化）
    /// - 必须同时清空 tx_hash
    /// - 并写入 need_service_fee = true 事实
    /// - 确保 scanner / recover 只基于有效事实工作
    ///
    /// 本方法是“事实回滚”，不是状态流转。
    /// 该方法不是重试控制，而是事实作废。
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许对尚未广播的交易调用（transaction_time IS NULL）
    /// - status 仅用于错误标注，不得用于流程推进
    /// - 📌 必须检查返回值 rows_affected()：
    ///   * rows_affected() == 0：表示事实已变更，无需处理
    ///   * rows_affected() == 1：表示成功作废事实
    ///   * 不建议直接忽略返回值
    pub async fn invalidate_raw_tx_need_service_fee<'a, E>(
        exec: E,
        trade_no: &str,
        status: Option<ApiCollectStatus>,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                raw_tx = NULL,
                tx_hash = NULL,
                building_at = NULL,
                service_fee_uploaded_at = NULL,
                need_service_fee = true,
                ever_needed_service_fee = true,
                status = COALESCE($2, status),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND transaction_time IS NULL
              AND last_broadcast_at IS NULL
        "#;

        let mut query = sqlx::query(sql).bind(trade_no);
        query = query.bind(status);

        let res = query.execute(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 作废当前 raw_tx 及其 tx_hash（仅用于重建，不触发补手续费语义）
    ///
    /// 语义：
    /// - 清空当前构建产物（raw_tx/tx_hash），允许后续重新构建
    /// - 不修改 need_service_fee / ever_needed_service_fee
    /// - 不重置 service_fee_* 事实
    ///
    /// 适用场景：
    /// - raw_tx 过期
    /// - 需要重新构建，但并非因为手续费不足
    pub async fn invalidate_raw_tx_for_rebuild<'a, E>(
        exec: E,
        trade_no: &str,
        status: Option<ApiCollectStatus>,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                raw_tx = NULL,
                tx_hash = NULL,
                building_at = NULL,
                status = COALESCE($2, status),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND transaction_time IS NULL
              AND last_broadcast_at IS NULL
        "#;

        let mut query = sqlx::query(sql).bind(trade_no);
        query = query.bind(status);

        let res = query.execute(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 作废当前 raw_tx 及其 tx_hash（用于广播后判定失联后的重建）
    pub async fn invalidate_raw_tx_for_rebroadcast<'a, E>(
        exec: E,
        trade_no: &str,
        status: Option<ApiCollectStatus>,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                raw_tx = NULL,
                tx_hash = NULL,
                building_at = NULL,
                last_broadcast_at = NULL,
                broadcast_uncertain_since_at = NULL,
                broadcast_uncertain_retry_count = 0,
                broadcast_uncertain_last_checked_at = NULL,
                broadcast_uncertain_reconciled_at = NULL,
                broadcast_uncertain_rebroadcast_count = 0,
                status = COALESCE($2, status),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND transaction_time IS NULL
              AND last_broadcast_at IS NOT NULL
        "#;

        let mut query = sqlx::query(sql).bind(trade_no);
        query = query.bind(status);

        let res = query.execute(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 兼容旧调用：默认语义为“需要补手续费”的事实回滚。
    pub async fn invalidate_raw_tx<'a, E>(
        exec: E,
        trade_no: &str,
        status: Option<ApiCollectStatus>,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        Self::invalidate_raw_tx_need_service_fee(exec, trade_no, status).await
    }

    /// 解决服务费需求标记
    ///
    /// ⚠️ 设计约束：
    /// - 仅允许在"外部事实已发生"的前提下调用（如 fee 到账）
    /// - 语义是：解除"需要服务费"的事实，允许重新构建
    ///
    /// ⚠️ 调用约定：
    /// - 必须由产生新事实的一方调用（如 fee mqtt 处理器）
    /// - 禁止在 scanner / worker / retry 逻辑中调用
    ///
    /// ⚠️ 铁律：
    /// - 本方法只修复 need_service_fee 事实
    /// - 不写 raw_tx / tx_hash
    /// - 不触发任何流程推进
    /// - Scanner 只会在后续扫描中自然推进
    pub async fn resolve_need_service_fee<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                need_service_fee = false,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND need_service_fee = true
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 清除服务费需求标记（recover 专用）
    ///
    /// 语义：
    /// - 修复"手续费不足"这一事实，使交易重新具备构建条件
    /// - 不做任何状态回滚，不保证一定继续推进
    ///
    /// 调用场景：
    /// - 手续费问题已解决，需要重新构建交易
    ///
    /// ⚠️ 铁律：
    /// - 本方法只修复 need_service_fee 事实
    /// - 不写 raw_tx / tx_hash
    /// - 不触发任何流程推进
    /// - Scanner 只会在后续扫描中自然推进
    pub async fn clear_need_service_fee<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                need_service_fee = false,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 标记已收到 SER TxRes 推送（AWM_ORDER_TRANS_RES）
    ///
    /// 语义：
    /// - 仅表示“SDK 已收到并持久化 SER 的交易执行结果推送”
    /// - 与链上确认（transaction_time）不是同一事实
    /// - 用于强顺序屏障：TX_RES ACK 禁止早于该事实发送
    ///
    /// 幂等约束：
    /// - 只允许写入一次（WHERE tx_res_received_at IS NULL）
    pub async fn update_tx_res_received_at<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                tx_res_received_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND tx_res_received_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 兼容保留：标记 Result ACK 尝试
    ///
    /// 当前 attempted_at 字段已移除，此方法保留 send_count 递增语义。
    pub async fn mark_result_ack_attempted<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                result_ack_send_count = result_ack_send_count + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND result_ack_sent_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 标记 Result ACK 确认（推进事实）
    ///
    /// 语义：
    /// - 防止重复确认（WHERE result_ack_sent_at IS NULL）
    pub async fn mark_result_ack_confirmed<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                result_ack_sent_at = COALESCE(
                    result_ack_sent_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND result_ack_sent_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 原子标记 Result ACK 已确认并标记链上终态
    ///
    /// 语义：
    /// - Result ACK 已成功发送到后端（result_ack_sent_at）
    /// - 同时标记链上终态（finished_at）
    /// - 单条 SQL 原子更新，防止 kill -9 产生"半完成事实"
    /// - WHERE 带旧事实约束，保证并发安全
    ///
    /// 写入顺序约束（不可逆）：
    /// raw_tx → tx_hash → transaction_time → finished_at
    pub async fn mark_result_ack_confirmed_and_chain_finished<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                result_ack_sent_at = COALESCE(
                    result_ack_sent_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                finished_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND result_ack_sent_at IS NULL
              AND finished_at IS NULL
              AND transaction_time IS NOT NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// ⚠️ Legacy shortcut
    /// ⚠️ Bypasses attempted/confirmed split
    /// ⚠️ DO NOT use in new code
    ///
    /// 标记 Result ACK 发送，并设置终态
    pub async fn mark_result_ack_sent<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                result_ack_send_count = result_ack_send_count + 1,
                result_ack_sent_at = COALESCE(
                    result_ack_sent_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND result_ack_sent_at IS NULL
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 标记手续费结果确认 ACK 已发送
    ///
    /// 语义：
    /// - 手续费结果确认 ACK 已成功发送到后端
    /// - 这是副作用完成的事实
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许调用一次（tx_fee_res_ack_sent_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_fee_res_ack_sent<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                tx_fee_res_ack_sent_at = COALESCE(
                    tx_fee_res_ack_sent_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND tx_fee_res_ack_sent_at IS NULL
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 兼容保留：标记服务费上传尝试
    ///
    /// 当前 attempted_at 字段已移除，此方法仅刷新 updated_at 以保持调用兼容。
    pub async fn mark_service_fee_attempted<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND service_fee_uploaded_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 标记已收到后端手续费订单（collect-side backend fee order fact）
    ///
    /// 语义：
    /// - 后端已下发手续费订单（AWM_ORDER_TRANS trade_type=3）
    /// - 这是 collect 侧前置事实，不代表服务费已上传
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许调用一次（service_fee_order_received_at IS NULL）
    /// - 由 MQTT fee-order handler 调用
    pub async fn mark_service_fee_order_received<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                service_fee_order_received_at = COALESCE(
                    service_fee_order_received_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND service_fee_order_received_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 标记服务费已上传
    ///
    /// 语义：
    /// - 服务费记录已成功上传到后端
    /// - 这是副作用完成的事实
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在服务费已上传的前提下调用
    /// - 仅允许调用一次（service_fee_uploaded_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_service_fee_uploaded<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                service_fee_uploaded_at = COALESCE(
                    service_fee_uploaded_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND service_fee_uploaded_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 原子标记链上终态（唯一合法 finished_at 写入口）
    ///
    /// 语义：
    /// - 链上已确认不可逆（success / failure）
    /// - 系统生命周期结束
    /// - 唯一合法写入 finished_at 的入口
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在链上事实已确认的前提下调用（transaction_time IS NOT NULL）
    /// - 仅允许调用一次（finished_at IS NULL）
    /// - 任何 ACK / upload / retry / worker 都不得写 finished_at
    /// - 只有“链终态确认模块”能调用此方法
    ///
    /// 📌 必须检查返回值 rows_affected()：
    ///   * rows_affected() == 0：表示事实已变更，无需处理
    ///   * rows_affected() == 1：表示成功标记链上终态
    ///   * 不建议直接忽略返回值
    pub async fn mark_chain_finished<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                finished_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND finished_at IS NULL
              AND (
                    transaction_time IS NOT NULL
                    OR (
                        transaction_time IS NULL
                        AND err_code IS NOT NULL
                        AND tx_exec_receipt_uploaded_at IS NOT NULL
                    )
              )
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// Mark successful broadcast execution
    ///
    /// Semantics:
    /// - Represents a successful broadcast attempt
    /// - NOT a chain confirmation
    /// - Idempotent, overwrite allowed
    pub async fn mark_broadcast_executed<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                broadcast_uncertain_since_at = NULL,
                broadcast_uncertain_retry_count = 0,
                broadcast_uncertain_last_checked_at = NULL,
                broadcast_uncertain_reconciled_at = NULL,
                broadcast_uncertain_rebroadcast_count = 0,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// Record EVM broadcast/recover uncertain observation.
    ///
    /// Semantics:
    /// - First uncertain fact sets broadcast_uncertain_since_at (COALESCE)
    /// - Every uncertain observation bumps retry_count and last_checked_at
    /// - Does NOT imply broadcast success
    pub async fn mark_broadcast_uncertain_attempt<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                broadcast_uncertain_since_at = COALESCE(
                    broadcast_uncertain_since_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                broadcast_uncertain_retry_count = COALESCE(broadcast_uncertain_retry_count, 0) + 1,
                broadcast_uncertain_last_checked_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND finished_at IS NULL
              AND err_code IS NULL
              AND transaction_time IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// Mark that uncertain timeout reconcile has been executed (at most once per lifecycle).
    pub async fn mark_broadcast_uncertain_reconciled<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                broadcast_uncertain_reconciled_at = COALESCE(
                    broadcast_uncertain_reconciled_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND finished_at IS NULL
              AND err_code IS NULL
              AND transaction_time IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// Record one automatic rebuild/rebroadcast retry after uncertain timeout.
    pub async fn mark_broadcast_uncertain_rebroadcast_attempted<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                broadcast_uncertain_rebroadcast_count = COALESCE(broadcast_uncertain_rebroadcast_count, 0) + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND finished_at IS NULL
              AND err_code IS NULL
              AND transaction_time IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// Clear uncertain tracking facts when the tx lifecycle reaches a stronger fact.
    pub async fn clear_broadcast_uncertain_tracking<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                broadcast_uncertain_since_at = NULL,
                broadcast_uncertain_retry_count = 0,
                broadcast_uncertain_last_checked_at = NULL,
                broadcast_uncertain_reconciled_at = NULL,
                broadcast_uncertain_rebroadcast_count = 0,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 扫描可能卡住的交易
    ///
    /// 事实条件：
    /// - finished_at IS NULL：系统生命周期未结束
    /// - err_code IS NULL：无错误
    /// - created_at < now() - interval '5 minutes'：至少等待 5 分钟
    /// - (order_ack_sent_at IS NOT NULL OR raw_tx IS NOT NULL OR last_broadcast_at IS NOT NULL)：有一定进展
    ///
    /// ⚠️ 重要约束：
    /// - 只返回可能卡住的交易
    /// - 使用 LIMIT 控制返回数量
    /// - ORDER BY created_at 优先处理 older 的交易
    pub async fn scan_possible_stuck<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_collect 
            WHERE finished_at IS NULL
            AND err_code IS NULL
            AND created_at < strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-5 minutes')
            AND (
                (order_ack_sent_at IS NOT NULL AND raw_tx IS NULL)
             OR (raw_tx IS NOT NULL AND last_broadcast_at IS NULL)
             OR (last_broadcast_at IS NOT NULL)
            )   
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiCollectEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 更新状态字段
    ///
    /// ⚠️ 仅由 recompute_and_update_status 调用
    /// ⚠️ 状态是派生字段，不是事实
    /// ⚠️ 不影响执行逻辑，仅用于显示
    pub async fn update_status<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiCollectStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                status = $2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(&status)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 兼容保留：标记交易执行回执上传尝试
    ///
    /// 当前 attempted_at 字段已移除，此方法仅刷新 updated_at 以保持调用兼容。
    pub async fn mark_tx_exec_receipt_attempted<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND tx_exec_receipt_uploaded_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 标记交易执行回执已上传
    ///
    /// 语义：
    /// - 交易执行回执已成功上传到后端
    /// - 这是副作用完成的事实
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在回执已上传的前提下调用
    /// - 仅允许调用一次（tx_exec_receipt_uploaded_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_exec_receipt_uploaded<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                tx_exec_receipt_uploaded_at = COALESCE(
                    tx_exec_receipt_uploaded_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND tx_exec_receipt_uploaded_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 扫描需要上传交易执行回执的交易
    ///
    /// 事实条件：
    /// - tx_exec_receipt_uploaded_at IS NULL：尚未上传执行回执
    /// - finished_at IS NULL：系统生命周期未结束
    /// - (transaction_time IS NOT NULL OR err_code IS NOT NULL)：
    ///     - 链上结果已确认
    ///     - 或出现终止型错误
    ///
    /// ⚠️ 架构铁律：
    /// - UploadTxExecReceipt =【执行行为回执】
    /// - 表示系统已执行 SendRawTx 并收到节点响应
    /// - 只允许在链上结果已确认或明确失败后进入扫描
    /// - 广播可见但结果未确定时不得上报
    /// - tx_hash 只是构建事实，不能作为执行回执 gate
    ///
    /// ⚠️ err_code 仍允许上传：
    /// - 属于行为事实补齐副作用
    /// - 不属于推进，不受 err_code 冻结
    ///
    /// ⚠️ scanner 冻结（等待 tx_hash 补齐）：
    /// - 若会构造 Success 回执（无 err_code 且已有执行证据），但 tx_hash 缺失
    /// - 则本地已知该回执无法成功上传，scanner 不应重复投递
    /// - 待后续事实补齐 tx_hash 后会自动重新进入扫描结果（无需显式解冻）
    pub async fn scan_need_tx_exec_receipt_upload<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_collect 
            WHERE finished_at IS NULL
            AND tx_exec_receipt_uploaded_at IS NULL
            AND (
                transaction_time IS NOT NULL
                OR err_code IS NOT NULL
            )
            AND NOT (
                err_code IS NULL
                AND transaction_time IS NOT NULL
                AND (
                    tx_hash IS NULL
                    OR trim(tx_hash) = ''
                )
            )
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiCollectEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 扫描需要发送订单 ACK 的交易
    ///
    /// 事实条件直接翻译：
    /// - order_ack_sent_at IS NULL：尚未发送订单 ACK
    /// - id IS NOT NULL：记录已存在
    ///
    /// ⚠️ 注意：
    /// - 不检查尝试中间态（attempted 语义不参与 Scanner 判断）
    /// - attempted 只用于 Worker / 运维观测
    pub async fn scan_need_order_ack<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_collect 
            WHERE order_ack_sent_at IS NULL
            AND finished_at IS NULL
            AND err_code IS NULL
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiCollectEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    pub async fn scan_need_recover<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_collect 
            WHERE tx_hash IS NOT NULL
            AND transaction_time IS NULL
            AND tx_exec_receipt_uploaded_at IS NULL
            AND finished_at IS NULL
            AND err_code IS NULL
            AND NOT (
                chain_code IN ('bnb','eth')
                AND raw_tx IS NOT NULL
                AND last_broadcast_at IS NULL
                AND broadcast_uncertain_since_at IS NULL
            )
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiCollectEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::ApiCollectDao;
    use crate::{
        SqliteContext, entities::api_collect::ApiCollectStatus,
        repositories::api_wallet::collect::ApiCollectRepo,
    };

    fn make_temp_dir(prefix: &str) -> String {
        let now =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("{prefix}_{pid}_{now}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().to_string()
    }

    #[tokio::test]
    async fn scan_confirmed_need_result_ack_requires_tx_res_received_at() {
        let dir = make_temp_dir("wallet_db_api_collect_scan_need_result_ack_gate");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        // record A: eligible
        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_TX_RES_A",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect SET transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ','now'), tx_res_received_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE trade_no = ?",
        )
        .bind("C_TX_RES_A")
        .execute(pool.as_ref())
        .await
        .unwrap();

        // record B: missing tx_res_received_at => excluded
        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_TX_RES_B",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect SET transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE trade_no = ?",
        )
        .bind("C_TX_RES_B")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records =
            ApiCollectDao::scan_confirmed_need_result_ack(pool.as_ref(), 100).await.unwrap();
        let trade_nos: Vec<String> = records.into_iter().map(|r| r.trade_no).collect();

        assert!(trade_nos.contains(&"C_TX_RES_A".to_string()));
        assert!(!trade_nos.contains(&"C_TX_RES_B".to_string()));
    }

    #[tokio::test]
    async fn scan_confirmed_need_service_fee_upload_requires_need_service_fee_only() {
        let dir = make_temp_dir("wallet_db_api_collect_scan_need_service_fee_uploaded_only");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_SERVICE_FEE_WAIT",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET need_service_fee = true,
                 service_fee_uploaded_at = NULL,
                 service_fee_order_received_at = NULL
             WHERE trade_no = ?",
        )
        .bind("C_SERVICE_FEE_WAIT")
        .execute(pool.as_ref())
        .await
        .unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_SERVICE_FEE_READY",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET need_service_fee = true,
                 service_fee_uploaded_at = NULL,
                 service_fee_order_received_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_SERVICE_FEE_READY")
        .execute(pool.as_ref())
        .await
        .unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_SERVICE_FEE_DONE",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET need_service_fee = true,
                 service_fee_order_received_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 service_fee_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_SERVICE_FEE_DONE")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records = ApiCollectDao::scan_confirmed_need_service_fee_upload(pool.as_ref(), 100)
            .await
            .unwrap();
        let trade_nos: Vec<String> = records.into_iter().map(|r| r.trade_no).collect();

        assert!(trade_nos.contains(&"C_SERVICE_FEE_WAIT".to_string()));
        assert!(trade_nos.contains(&"C_SERVICE_FEE_READY".to_string()));
        assert!(!trade_nos.contains(&"C_SERVICE_FEE_DONE".to_string()));
    }

    #[tokio::test]
    async fn scan_can_build_requires_fee_cycle_cleared() {
        let dir = make_temp_dir("wallet_db_api_collect_scan_can_build_stale_fee");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_CAN_BUILD_STALE",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = true,
                 service_fee_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_CAN_BUILD_STALE")
        .execute(pool.as_ref())
        .await
        .unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_CAN_BUILD_READY",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = false
             WHERE trade_no = ?",
        )
        .bind("C_CAN_BUILD_READY")
        .execute(pool.as_ref())
        .await
        .unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_CAN_BUILD_BLOCKED",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = true
             WHERE trade_no = ?",
        )
        .bind("C_CAN_BUILD_BLOCKED")
        .execute(pool.as_ref())
        .await
        .unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_CAN_BUILD_ACTIVE",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = false,
                 building_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_CAN_BUILD_ACTIVE")
        .execute(pool.as_ref())
        .await
        .unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_CAN_BUILD_STALE_BUILDING",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = false,
                 building_at = datetime('now', '-31 seconds')
             WHERE trade_no = ?",
        )
        .bind("C_CAN_BUILD_STALE_BUILDING")
        .execute(pool.as_ref())
        .await
        .unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_CAN_BUILD_RECOVERED",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = false,
                 transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_CAN_BUILD_RECOVERED")
        .execute(pool.as_ref())
        .await
        .unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_CAN_BUILD_FINISHED",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = false,
                 finished_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_CAN_BUILD_FINISHED")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records = ApiCollectDao::scan_can_build(pool.as_ref(), 100).await.unwrap();
        let trade_nos: Vec<String> = records.into_iter().map(|r| r.trade_no).collect();

        assert!(!trade_nos.contains(&"C_CAN_BUILD_STALE".to_string()));
        assert!(trade_nos.contains(&"C_CAN_BUILD_READY".to_string()));
        assert!(!trade_nos.contains(&"C_CAN_BUILD_BLOCKED".to_string()));
        assert!(trade_nos.contains(&"C_CAN_BUILD_ACTIVE".to_string()));
        assert!(trade_nos.contains(&"C_CAN_BUILD_STALE_BUILDING".to_string()));
        assert!(!trade_nos.contains(&"C_CAN_BUILD_RECOVERED".to_string()));
        assert!(!trade_nos.contains(&"C_CAN_BUILD_FINISHED".to_string()));
    }

    #[tokio::test]
    async fn scan_can_build_blocks_completed_fee_cycle_until_fee_ack_sent() {
        let dir = make_temp_dir("wallet_db_api_collect_scan_can_build_fee_ack");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_CAN_BUILD_FEE_ACK_BLOCKED",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = false,
                 ever_needed_service_fee = true,
                 tx_fee_res_ack_sent_at = NULL,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_CAN_BUILD_FEE_ACK_BLOCKED")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records = ApiCollectDao::scan_can_build(pool.as_ref(), 100).await.unwrap();
        let trade_nos: Vec<String> = records.into_iter().map(|r| r.trade_no).collect();

        assert!(!trade_nos.contains(&"C_CAN_BUILD_FEE_ACK_BLOCKED".to_string()));
    }

    #[tokio::test]
    async fn update_building_at_claims_slot_once() {
        let dir = make_temp_dir("wallet_db_api_collect_update_building_at");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_BUILDING_CLAIM",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        let first =
            ApiCollectDao::update_building_at(pool.as_ref(), "C_BUILDING_CLAIM").await.unwrap();
        let second =
            ApiCollectDao::update_building_at(pool.as_ref(), "C_BUILDING_CLAIM").await.unwrap();

        assert_eq!(first, 1);
        assert_eq!(second, 0);
    }

    #[tokio::test]
    async fn clear_building_at_releases_slot() {
        let dir = make_temp_dir("wallet_db_api_collect_clear_building_at");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_BUILDING_CLEAR",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        let claimed =
            ApiCollectDao::update_building_at(pool.as_ref(), "C_BUILDING_CLEAR").await.unwrap();
        assert_eq!(claimed, 1);

        let cleared =
            ApiCollectDao::clear_building_at(pool.as_ref(), "C_BUILDING_CLEAR").await.unwrap();
        assert_eq!(cleared, 1);

        let rec =
            ApiCollectRepo::get_api_collect_by_trade_no(&pool, "C_BUILDING_CLEAR").await.unwrap();
        assert!(rec.building_at.is_none());
    }

    #[tokio::test]
    async fn scan_confirmed_need_tx_fee_res_ack_requires_fee_cycle_cleared() {
        let dir = make_temp_dir("wallet_db_api_collect_scan_need_tx_fee_res_ack_stale");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_TX_FEE_RES_STALE",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = true,
                 service_fee_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 ever_needed_service_fee = true
             WHERE trade_no = ?",
        )
        .bind("C_TX_FEE_RES_STALE")
        .execute(pool.as_ref())
        .await
        .unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_TX_FEE_RES_READY",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = false,
                 ever_needed_service_fee = true
             WHERE trade_no = ?",
        )
        .bind("C_TX_FEE_RES_READY")
        .execute(pool.as_ref())
        .await
        .unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_TX_FEE_RES_BLOCKED",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = true,
                 ever_needed_service_fee = true
             WHERE trade_no = ?",
        )
        .bind("C_TX_FEE_RES_BLOCKED")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records =
            ApiCollectDao::scan_confirmed_need_tx_fee_res_ack(pool.as_ref(), 100).await.unwrap();
        let trade_nos: Vec<String> = records.into_iter().map(|r| r.trade_no).collect();

        assert!(!trade_nos.contains(&"C_TX_FEE_RES_STALE".to_string()));
        assert!(trade_nos.contains(&"C_TX_FEE_RES_READY".to_string()));
        assert!(!trade_nos.contains(&"C_TX_FEE_RES_BLOCKED".to_string()));
    }

    #[tokio::test]
    async fn scan_need_tx_exec_receipt_upload_allows_transaction_time_without_last_broadcast() {
        let dir = make_temp_dir("wallet_db_api_collect_scan_need_receipt_tx_time");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_RECEIPT_TX_TIME",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 tx_hash = '0xtesthash'
             WHERE trade_no = ?",
        )
        .bind("C_RECEIPT_TX_TIME")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records =
            ApiCollectDao::scan_need_tx_exec_receipt_upload(pool.as_ref(), 100).await.unwrap();
        let trade_nos: Vec<String> = records.into_iter().map(|r| r.trade_no).collect();

        assert!(trade_nos.contains(&"C_RECEIPT_TX_TIME".to_string()));
    }

    #[tokio::test]
    async fn scan_need_tx_exec_receipt_upload_excludes_broadcast_visible_pending() {
        let dir = make_temp_dir("wallet_db_api_collect_scan_need_receipt_pending");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "c",
            None,
            "s",
            "C_RECEIPT_PENDING",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 tx_hash = '0xtesthash'
             WHERE trade_no = ?",
        )
        .bind("C_RECEIPT_PENDING")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records =
            ApiCollectDao::scan_need_tx_exec_receipt_upload(pool.as_ref(), 100).await.unwrap();
        assert!(!records.iter().any(|r| r.trade_no == "C_RECEIPT_PENDING"));
    }

    #[tokio::test]
    async fn scan_need_tx_exec_receipt_upload_freezes_success_missing_hash() {
        let dir = make_temp_dir("wallet_db_api_collect_scan_receipt_freeze_missing_hash");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "tron",
            None,
            "USDT",
            "C_RECEIPT_FREEZE_EMPTY_HASH",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 tx_hash = ''
             WHERE trade_no = ?",
        )
        .bind("C_RECEIPT_FREEZE_EMPTY_HASH")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records =
            ApiCollectDao::scan_need_tx_exec_receipt_upload(pool.as_ref(), 100).await.unwrap();
        let trade_nos: Vec<String> = records.into_iter().map(|r| r.trade_no).collect();
        assert!(!trade_nos.contains(&"C_RECEIPT_FREEZE_EMPTY_HASH".to_string()));
    }

    #[tokio::test]
    async fn scan_need_tx_exec_receipt_upload_unfreezes_after_hash_backfill() {
        let dir = make_temp_dir("wallet_db_api_collect_scan_receipt_unfreeze_after_backfill");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "tron",
            None,
            "USDT",
            "C_RECEIPT_UNFREEZE_AFTER_BACKFILL",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 tx_hash = ''
             WHERE trade_no = ?",
        )
        .bind("C_RECEIPT_UNFREEZE_AFTER_BACKFILL")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let before =
            ApiCollectDao::scan_need_tx_exec_receipt_upload(pool.as_ref(), 100).await.unwrap();
        assert!(!before.iter().any(|r| r.trade_no == "C_RECEIPT_UNFREEZE_AFTER_BACKFILL"));

        let rows = ApiCollectDao::backfill_tx_hash_if_missing(
            pool.as_ref(),
            "C_RECEIPT_UNFREEZE_AFTER_BACKFILL",
            "0xbackfilled",
        )
        .await
        .unwrap();
        assert_eq!(rows, 1);

        let after =
            ApiCollectDao::scan_need_tx_exec_receipt_upload(pool.as_ref(), 100).await.unwrap();
        assert!(after.iter().any(|r| r.trade_no == "C_RECEIPT_UNFREEZE_AFTER_BACKFILL"));
    }

    #[tokio::test]
    async fn scan_need_tx_exec_receipt_upload_fail_path_not_frozen_by_missing_hash() {
        let dir = make_temp_dir("wallet_db_api_collect_scan_receipt_fail_not_frozen");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "tron",
            None,
            "USDT",
            "C_RECEIPT_FAIL_EMPTY_HASH_ALLOWED",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET err_code = 6099,
                 tx_hash = ''
             WHERE trade_no = ?",
        )
        .bind("C_RECEIPT_FAIL_EMPTY_HASH_ALLOWED")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records =
            ApiCollectDao::scan_need_tx_exec_receipt_upload(pool.as_ref(), 100).await.unwrap();
        assert!(records.iter().any(|r| r.trade_no == "C_RECEIPT_FAIL_EMPTY_HASH_ALLOWED"));
    }

    #[tokio::test]
    async fn invalidate_raw_tx_need_service_fee_resets_service_fee_cycle_facts() {
        let dir = make_temp_dir("wallet_db_api_collect_invalidate_resets_fee_cycle");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "tron",
            None,
            "USDT",
            "C_INV_RESET_FEE_CYCLE",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET raw_tx = 'raw',
                 tx_hash = 'hash',
                 building_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 service_fee_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 tx_fee_res_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = false,
                 ever_needed_service_fee = true
             WHERE trade_no = ?",
        )
        .bind("C_INV_RESET_FEE_CYCLE")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let rows = ApiCollectDao::invalidate_raw_tx_need_service_fee(
            pool.as_ref(),
            "C_INV_RESET_FEE_CYCLE",
            Some(ApiCollectStatus::InsufficientBalance),
        )
        .await
        .unwrap();
        assert_eq!(rows, 1);

        let rec =
            ApiCollectDao::get_api_collect_by_trade_no(pool.as_ref(), "C_INV_RESET_FEE_CYCLE")
                .await
                .unwrap();
        assert!(rec.raw_tx.is_none());
        assert!(rec.tx_hash.is_none());
        assert!(rec.building_at.is_none());
        assert_eq!(rec.need_service_fee, Some(true));
        assert!(rec.ever_needed_service_fee);
        assert!(rec.service_fee_uploaded_at.is_none());
        assert!(rec.tx_fee_res_ack_sent_at.is_some());
    }

    #[tokio::test]
    async fn invalidate_raw_tx_for_rebuild_preserves_service_fee_facts_and_need_flag() {
        let dir = make_temp_dir("wallet_db_api_collect_invalidate_for_rebuild_preserves_fee_facts");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "tron",
            None,
            "USDT",
            "C_INV_REBUILD_ONLY",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET raw_tx = 'raw',
                 tx_hash = 'hash',
                 building_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 service_fee_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 need_service_fee = false,
                 ever_needed_service_fee = true
             WHERE trade_no = ?",
        )
        .bind("C_INV_REBUILD_ONLY")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let rows =
            ApiCollectDao::invalidate_raw_tx_for_rebuild(pool.as_ref(), "C_INV_REBUILD_ONLY", None)
                .await
                .unwrap();
        assert_eq!(rows, 1);

        let rec = ApiCollectDao::get_api_collect_by_trade_no(pool.as_ref(), "C_INV_REBUILD_ONLY")
            .await
            .unwrap();
        assert!(rec.raw_tx.is_none());
        assert!(rec.tx_hash.is_none());
        assert!(rec.building_at.is_none());
        assert_eq!(rec.need_service_fee, Some(false));
        assert!(rec.ever_needed_service_fee);
        assert!(rec.service_fee_uploaded_at.is_some());
    }

    #[tokio::test]
    async fn invalidate_raw_tx_skips_when_last_broadcast_exists() {
        let dir = make_temp_dir("wallet_db_api_collect_invalidate_broadcast_guard");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "tron",
            None,
            "USDT",
            "C_INV_BROADCAST_GUARD",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET raw_tx = 'raw',
                 tx_hash = 'hash',
                 last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_INV_BROADCAST_GUARD")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let rows = ApiCollectDao::invalidate_raw_tx_need_service_fee(
            pool.as_ref(),
            "C_INV_BROADCAST_GUARD",
            Some(ApiCollectStatus::InsufficientBalance),
        )
        .await
        .unwrap();
        assert_eq!(rows, 0);

        let rec =
            ApiCollectDao::get_api_collect_by_trade_no(pool.as_ref(), "C_INV_BROADCAST_GUARD")
                .await
                .unwrap();
        assert_eq!(rec.raw_tx.as_deref(), Some("raw"));
        assert_eq!(rec.tx_hash.as_deref(), Some("hash"));
        assert!(rec.last_broadcast_at.is_some());
        assert_ne!(rec.need_service_fee, Some(true));
    }

    #[tokio::test]
    async fn invalidate_raw_tx_skips_when_transaction_time_exists() {
        let dir = make_temp_dir("wallet_db_api_collect_invalidate_tx_time_guard");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "tron",
            None,
            "USDT",
            "C_INV_TX_TIME_GUARD",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET raw_tx = 'raw',
                 tx_hash = 'hash',
                 transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_INV_TX_TIME_GUARD")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let rows = ApiCollectDao::invalidate_raw_tx_need_service_fee(
            pool.as_ref(),
            "C_INV_TX_TIME_GUARD",
            Some(ApiCollectStatus::InsufficientBalance),
        )
        .await
        .unwrap();
        assert_eq!(rows, 0);

        let rec = ApiCollectDao::get_api_collect_by_trade_no(pool.as_ref(), "C_INV_TX_TIME_GUARD")
            .await
            .unwrap();
        assert_eq!(rec.raw_tx.as_deref(), Some("raw"));
        assert_eq!(rec.tx_hash.as_deref(), Some("hash"));
        assert!(rec.transaction_time.is_some());
        assert_ne!(rec.need_service_fee, Some(true));
    }

    #[tokio::test]
    async fn backfill_tx_hash_if_missing_updates_when_execution_fact_exists() {
        let dir = make_temp_dir("wallet_db_api_collect_backfill_tx_hash_ok");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "tron",
            None,
            "USDT",
            "C_BACKFILL_TX_HASH_OK",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET tx_hash = '',
                 last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_BACKFILL_TX_HASH_OK")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let rows = ApiCollectDao::backfill_tx_hash_if_missing(
            pool.as_ref(),
            "C_BACKFILL_TX_HASH_OK",
            "0xabc123",
        )
        .await
        .unwrap();
        assert_eq!(rows, 1);

        let rec =
            ApiCollectDao::get_api_collect_by_trade_no(pool.as_ref(), "C_BACKFILL_TX_HASH_OK")
                .await
                .unwrap();
        assert_eq!(rec.tx_hash.as_deref(), Some("0xabc123"));
    }

    #[tokio::test]
    async fn backfill_tx_hash_if_missing_does_not_override_existing_non_empty_hash() {
        let dir = make_temp_dir("wallet_db_api_collect_backfill_tx_hash_no_override");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "tron",
            None,
            "USDT",
            "C_BACKFILL_TX_HASH_NO_OVERRIDE",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET tx_hash = 'existing_hash',
                 last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_BACKFILL_TX_HASH_NO_OVERRIDE")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let rows = ApiCollectDao::backfill_tx_hash_if_missing(
            pool.as_ref(),
            "C_BACKFILL_TX_HASH_NO_OVERRIDE",
            "0xnewhash",
        )
        .await
        .unwrap();
        assert_eq!(rows, 0);

        let rec = ApiCollectDao::get_api_collect_by_trade_no(
            pool.as_ref(),
            "C_BACKFILL_TX_HASH_NO_OVERRIDE",
        )
        .await
        .unwrap();
        assert_eq!(rec.tx_hash.as_deref(), Some("existing_hash"));
    }

    #[tokio::test]
    async fn backfill_tx_hash_if_missing_requires_execution_evidence() {
        let dir = make_temp_dir("wallet_db_api_collect_backfill_tx_hash_requires_fact");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "from",
            "to",
            "0",
            "v",
            "tron",
            None,
            "USDT",
            "C_BACKFILL_TX_HASH_NEEDS_FACT",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        let rows = ApiCollectDao::backfill_tx_hash_if_missing(
            pool.as_ref(),
            "C_BACKFILL_TX_HASH_NEEDS_FACT",
            "0xhash",
        )
        .await
        .unwrap();
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn find_candidates_for_acct_change_repair_matches_missing_hash_and_null_tx_time() {
        let dir = make_temp_dir("wallet_db_api_collect_find_candidates_missing_hash");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "TFromAddr",
            "TToAddr",
            "0",
            "499",
            "tron",
            Some("TTokenAddr".to_string()),
            "USDT",
            "C_ACCT_CHANGE_CANDIDATE_OK",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET tx_hash = '',
                 transaction_time = NULL,
                 last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_ACCT_CHANGE_CANDIDATE_OK")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let recs = ApiCollectDao::find_candidates_for_acct_change_repair(
            pool.as_ref(),
            "tron",
            "TFromAddr",
            "TToAddr",
            Some("TTokenAddr"),
            "USDT",
            10,
        )
        .await
        .unwrap();

        let trade_nos: Vec<String> = recs.into_iter().map(|r| r.trade_no).collect();
        assert!(trade_nos.contains(&"C_ACCT_CHANGE_CANDIDATE_OK".to_string()));
    }

    #[tokio::test]
    async fn find_candidates_for_acct_change_repair_excludes_rows_without_execution_evidence() {
        let dir = make_temp_dir("wallet_db_api_collect_find_candidates_no_exec_evidence");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "TFromAddr2",
            "TToAddr2",
            "0",
            "499",
            "tron",
            Some("TTokenAddr2".to_string()),
            "USDT",
            "C_ACCT_CHANGE_NO_EXEC_EVIDENCE",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET tx_hash = '',
                 transaction_time = NULL,
                 last_broadcast_at = NULL
             WHERE trade_no = ?",
        )
        .bind("C_ACCT_CHANGE_NO_EXEC_EVIDENCE")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let recs = ApiCollectDao::find_candidates_for_acct_change_repair(
            pool.as_ref(),
            "tron",
            "TFromAddr2",
            "TToAddr2",
            Some("TTokenAddr2"),
            "USDT",
            10,
        )
        .await
        .unwrap();

        assert!(recs.is_empty());
    }

    #[tokio::test]
    async fn find_candidates_for_acct_change_repair_excludes_rows_without_repair_need() {
        let dir = make_temp_dir("wallet_db_api_collect_find_candidates_no_repair");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "n",
            "EFromAddr",
            "EToAddr",
            "0",
            "10",
            "eth",
            None,
            "ETH",
            "C_ACCT_CHANGE_CANDIDATE_NO_REPAIR",
            2,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET tx_hash = '0xabc',
                 transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_ACCT_CHANGE_CANDIDATE_NO_REPAIR")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let recs = ApiCollectDao::find_candidates_for_acct_change_repair(
            pool.as_ref(),
            "eth",
            "EFromAddr",
            "EToAddr",
            None,
            "ETH",
            10,
        )
        .await
        .unwrap();

        assert!(recs.is_empty());
    }

    #[tokio::test]
    async fn scan_need_recover_includes_broadcast_visible_pending_collect_rows() {
        let dir = make_temp_dir("wallet_db_api_collect_scan_need_recover_visible");
        let ctx = SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
        let pool = ctx.into_transaction_db_pool().unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "collect",
            "from",
            "to",
            "1.12",
            "digest",
            "eth",
            None,
            "USDC",
            "C_RECOVER_VISIBLE",
            2,
            ApiCollectStatus::SendingTx,
            1,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 raw_tx = '{\"tx\":true}',
                 tx_hash = '0xvisible',
                 last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_RECOVER_VISIBLE")
        .execute(pool.as_ref())
        .await
        .unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "collect",
            "from",
            "to",
            "1.12",
            "digest",
            "eth",
            None,
            "USDC",
            "C_RECOVER_UPLOADED",
            2,
            ApiCollectStatus::SendingTx,
            1,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 raw_tx = '{\"tx\":true}',
                 tx_hash = '0xuploaded',
                 last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 tx_exec_receipt_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_RECOVER_UPLOADED")
        .execute(pool.as_ref())
        .await
        .unwrap();

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "uid",
            "collect",
            "from",
            "to",
            "1.12",
            "digest",
            "eth",
            None,
            "USDC",
            "C_RECOVER_PREREADY",
            2,
            ApiCollectStatus::SendingTx,
            1,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 raw_tx = '{\"tx\":true}',
                 tx_hash = '0xpreready',
                 updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind("C_RECOVER_PREREADY")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let rows = ApiCollectRepo::scan_need_recover(&pool, 10).await.unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|r| r.trade_no).collect();

        assert!(trade_nos.contains(&"C_RECOVER_VISIBLE".to_string()));
        assert!(!trade_nos.contains(&"C_RECOVER_UPLOADED".to_string()));
        assert!(!trade_nos.contains(&"C_RECOVER_PREREADY".to_string()));
    }
}
