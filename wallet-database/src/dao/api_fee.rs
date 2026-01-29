use crate::{
    DbPool,
    entities::api_fee::{ApiFeeEntity, ApiFeeStatus, FeeCreatedFact},
};
use sqlx::{Executor, Row, Sqlite};

// ⚠️ finished_at 为链终态事实字段
// ⚠️ 除 mark_chain_finished 外，禁止任何 UPDATE 语句写入 finished_at
// ⚠️ 未来 code review 时，搜索 `finished_at =` 并拒绝除 mark_chain_finished 外的所有情况

pub(crate) struct ApiFeeDao;

impl ApiFeeDao {
    pub async fn all_api_fee<'a, E>(exec: E, uid: &str) -> Result<Vec<ApiFeeEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"SELECT * FROM api_fee where uid = ?"#;
        let result = sqlx::query_as::<_, ApiFeeEntity>(sql)
            .bind(uid)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    pub async fn page_api_fee<'a, E>(
        exec: E,
        page: i64,
        page_size: i64,
    ) -> Result<(i64, Vec<ApiFeeEntity>), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Clone,
    {
        let count_sql = "SELECT count(*) FROM api_fee";
        let count = sqlx::query_scalar::<_, i64>(count_sql)
            .fetch_one(exec.clone())
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        let sql = "SELECT * FROM api_fee ORDER BY created_at DESC LIMIT ? OFFSET ?";
        let res = sqlx::query_as::<_, ApiFeeEntity>(sql)
            .bind(page_size)
            .bind(page)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok((count, res))
    }

    pub async fn page_api_fee_with_status<'a, E>(
        exec: E,
        _page: i64,
        page_size: i64,
        vec_status: &[ApiFeeStatus],
    ) -> Result<(i64, Vec<ApiFeeEntity>), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Clone,
    {
        let placeholders = vec_status.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let count_sql = format!("SELECT count(*) FROM api_fee where status in ({})", placeholders);
        let sql = format!(
            "SELECT * FROM api_fee where status in ({}) ORDER BY id ASC LIMIT ?",
            placeholders
        );

        let mut query = sqlx::query_scalar::<_, i64>(&count_sql);
        for status in vec_status {
            query = query.bind(status);
        }
        let count =
            query.fetch_one(exec.clone()).await.map_err(|e| crate::Error::Database(e.into()))?;

        let mut query = sqlx::query_as::<_, ApiFeeEntity>(&sql);
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

    pub async fn get_api_fee_by_trade_no<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<ApiFeeEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_fee WHERE trade_no = ?";
        let res = sqlx::query_as::<_, ApiFeeEntity>(sql)
            .bind(trade_no)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    pub async fn get_api_fee_by_trade_no_status<'a, E>(
        exec: E,
        trade_no: &str,
        vec_status: &[ApiFeeStatus],
    ) -> Result<ApiFeeEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let placeholders = vec_status.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql =
            format!("SELECT * FROM api_fee where trade_no = ? AND status in ({})", placeholders);
        let mut query = sqlx::query_as::<_, ApiFeeEntity>(&sql).bind(trade_no);
        for status in vec_status {
            query = query.bind(status);
        }
        let res = query.fetch_one(exec).await.map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    pub async fn add<'a, E>(exec: E, api_fee: FeeCreatedFact) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO api_fee
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
                status,
                created_at)
            VALUES
                ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT(trade_no) DO UPDATE SET
                updated_at          = strftime('%Y-%m-%dT%H:%M:%SZ','now')
        "#;

        let res = sqlx::query(sql)
            .bind(&api_fee.uid)
            .bind(&api_fee.name)
            .bind(&api_fee.from_addr)
            .bind(&api_fee.to_addr)
            .bind(&api_fee.value)
            .bind(&api_fee.validate)
            .bind(&api_fee.chain_code)
            .bind(api_fee.token_addr)
            .bind(&api_fee.symbol)
            .bind(&api_fee.trade_no)
            .bind(api_fee.trade_type)
            .bind(api_fee.status)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        tracing::info!(xx=%res.rows_affected(), "tx fee api");
        Ok(())
    }

    pub async fn update_status_and_err<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiFeeStatus,
        err_code: u32,
        err_msg: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
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

    pub async fn update_next_status<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiFeeStatus,
        next_status: ApiFeeStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
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

    pub async fn update_tx_status_nonce(
        pool: &DbPool,
        from_addr: &str,
        chain_code: &str,
        trade_no: &str,
        nonce: i64,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiFeeStatus,
    ) -> Result<(), crate::Error> {
        let mut tx = pool.begin().await.map_err(|e| crate::Error::Database(e.into()))?;
        let sql = r#"
            UPDATE api_fee
            SET
                tx_hash = $2,
                nonce = $3,
                resource_consume = $4,
                transaction_fee = $5,
                status = $6,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;

        sqlx::query(sql)
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

        let nonce = sqlx::query_scalar::<_, i32>(sql)
            .bind(from_addr)
            .bind(chain_code)
            .bind(nonce)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        tx.commit().await.map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn update_tx_status<'a, E>(
        exec: E,
        trade_no: &str,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiFeeStatus,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                tx_hash = $2,
                resource_consume = $3,
                transaction_fee = $4,
                status = $5,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;

        sqlx::query(sql)
            .bind(trade_no)
            .bind(tx_hash)
            .bind(resource_consume)
            .bind(transaction_fee)
            .bind(&status)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn update_post_tx_count<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiFeeStatus,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                post_tx_count = MIN(post_tx_count + 1, 63),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1 and status = $2
        "#;

        sqlx::query(sql)
            .bind(trade_no)
            .bind(&status)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn update_post_confirm_tx_count<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiFeeStatus,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                post_confirm_tx_count = MIN(post_confirm_tx_count + 1, 63),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1 and status = $2
        "#;
        sqlx::query(sql)
            .bind(trade_no)
            .bind(&status)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn set_tx_ack_sent<'a, E>(exec: E, trade_no: &str) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                tx_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;
        sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn set_tx_res_ack_sent<'a, E>(exec: E, trade_no: &str) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                tx_res_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;
        sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
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
            SELECT tx_ack_sent_at, tx_res_ack_sent_at
            FROM api_fee
            WHERE trade_no = $1
        "#;
        let row = sqlx::query(sql)
            .bind(trade_no)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        if let Some(row) = row {
            let tx_ack_sent_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>> =
                row.try_get(0).map_err(|e| crate::Error::Database(e.into()))?;
            let tx_res_ack_sent_at: Option<
                sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
            > = row.try_get(1).map_err(|e| crate::Error::Database(e.into()))?;

            Ok((tx_ack_sent_at, tx_res_ack_sent_at))
        } else {
            Ok((None, None))
        }
    }

    /// 标记交易 ACK 尝试（行为事实）
    ///
    /// 语义：
    /// - 只记录第一次尝试时间（COALESCE 幂等写）
    /// - 发送成功后不再变化（WHERE tx_ack_sent_at IS NULL）
    /// - 这是"行为事实"，不是"推进事实"
    pub async fn mark_tx_ack_attempted<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                tx_ack_attempted_at = COALESCE(
                    tx_ack_attempted_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND tx_ack_sent_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 标记交易 ACK 已发送（推进事实）
    ///
    /// 语义：
    /// - 交易 ACK 已成功发送到后端
    /// - 这是副作用完成的事实
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在交易 ACK 已尝试的前提下调用
    /// - 仅允许调用一次（tx_ack_sent_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_ack_sent<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                tx_ack_sent_at = COALESCE(
                    tx_ack_sent_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                tx_ack_attempted_at = COALESCE(
                    tx_ack_attempted_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND tx_ack_sent_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 标记交易执行回执上传尝试（行为事实）
    ///
    /// 语义：
    /// - 只记录第一次尝试时间（COALESCE 幂等写）
    /// - 上传成功后不再变化（WHERE tx_exec_receipt_uploaded_at IS NULL）
    /// - 这是"行为事实"，不是"推进事实"
    pub async fn mark_tx_exec_receipt_attempted<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                tx_exec_receipt_attempted_at = COALESCE(
                    tx_exec_receipt_attempted_at,
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
            UPDATE api_fee
            SET
                tx_exec_receipt_uploaded_at = COALESCE(
                    tx_exec_receipt_uploaded_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                tx_exec_receipt_attempted_at = COALESCE(
                    tx_exec_receipt_attempted_at,
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

    /// 标记交易结果 ACK 尝试（行为事实）
    ///
    /// 语义：
    /// - 只记录第一次尝试时间（COALESCE 幂等写）
    /// - 确认后不再变化
    /// - 这是"行为事实"，不是"推进事实"
    pub async fn mark_tx_res_ack_attempted<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                tx_res_ack_attempted_at = COALESCE(
                    tx_res_ack_attempted_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND tx_res_ack_sent_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 标记交易结果 ACK 已发送（推进事实）
    ///
    /// 语义：
    /// - 交易结果 ACK 已成功发送到后端
    /// - 这是副作用完成的事实
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在交易结果 ACK 已尝试的前提下调用
    /// - 仅允许调用一次（tx_res_ack_sent_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_res_ack_sent<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                tx_res_ack_sent_at = COALESCE(
                    tx_res_ack_sent_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND tx_res_ack_sent_at IS NULL
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
    /// - 这是"广播成功 → 链上确认"的不可逆事实跃迁
    /// - 单条 SQL 原子更新，防止 kill -9 产生"半完成事实"
    /// - WHERE 带旧事实约束，保证并发安全
    ///
    /// 写入顺序约束（不可逆）：
    /// raw_tx → tx_hash → transaction_time → finished_at
    pub async fn mark_chain_finished<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                finished_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND finished_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 扫描需要发送交易 ACK 的交易
    ///
    /// 事实条件直接翻译：
    /// - tx_ack_sent_at IS NULL：尚未发送交易 ACK
    /// - id IS NOT NULL：记录已存在
    ///
    /// ⚠️ 注意：
    /// - 不检查 tx_ack_attempted_at（这是行为事实，不参与 Scanner 判断）
    /// - attempted 只用于 Worker / 运维观测
    pub async fn scan_need_tx_ack<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiFeeEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_fee 
            WHERE tx_ack_sent_at IS NULL
            AND raw_tx IS NULL
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiFeeEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 扫描可构建的交易：raw_tx为空
    ///
    /// ⚠️ 核心事实驱动原则：
    /// - 只基于不可逆事实字段(raw_tx)决策
    /// - 不依赖时间字段(building_at)进行决策
    /// - 并发通过raw_tx写入唯一性保证
    ///
    /// ⚠️ 强顺序屏障：
    /// - BuildTx 必须发生在 Tx ACK 之后
    /// - 禁止移除 tx_ack_sent_at 条件，否则会破坏强顺序保证
    pub async fn scan_can_build<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiFeeEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            -- ⚠️ 强顺序屏障：
            -- BuildTx 必须发生在 Tx ACK 之后
            -- 禁止移除 tx_ack_sent_at 条件，否则会破坏强顺序保证
            SELECT * FROM api_fee 
            WHERE tx_ack_sent_at IS NOT NULL
            AND raw_tx IS NULL 
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiFeeEntity>(sql)
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
    ) -> Result<Vec<ApiFeeEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_fee 
            WHERE raw_tx IS NOT NULL 
            AND transaction_time IS NULL 
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiFeeEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 扫描需要上传交易执行回执的交易
    ///
    /// 事实条件直接翻译：
    /// - last_broadcast_at IS NOT NULL：交易已成功广播
    /// - finished_at IS NULL：系统生命周期未结束
    /// - tx_exec_receipt_uploaded_at IS NULL：尚未上传执行回执
    pub async fn scan_need_tx_exec_receipt_upload<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiFeeEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_fee 
            WHERE last_broadcast_at IS NOT NULL 
            AND finished_at IS NULL
            AND tx_exec_receipt_uploaded_at IS NULL
            ORDER BY last_broadcast_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiFeeEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 扫描已确认且需要发送交易结果 ACK 的交易
    ///
    /// 事实条件直接翻译：
    /// - tx_exec_receipt_uploaded_at IS NOT NULL：交易执行回执已上传
    /// - finished_at IS NULL：系统生命周期未结束
    /// - tx_res_ack_sent_at IS NULL：尚未发送交易结果 ACK（推进事实）
    ///
    /// ⚠️ 强顺序屏障：
    /// - TxResAck 必须发生在 TxExecReceipt 上传之后
    /// - 禁止使用 transaction_time 作为前置条件（共享前提事实）
    ///
    /// ⚠️ 注意：
    /// - 不检查 tx_res_ack_attempted_at（这是行为事实，不参与 Scanner 判断）
    /// - attempted 只用于 Worker / 运维观测
    pub async fn scan_confirmed_need_tx_res_ack<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiFeeEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            -- ⚠️ 强顺序屏障：
            -- TxResAck 必须发生在 TxExecReceipt 上传之后
            -- 禁止使用 transaction_time 作为前置条件（共享前提事实）
            SELECT * FROM api_fee 
            WHERE tx_exec_receipt_uploaded_at IS NOT NULL
            AND finished_at IS NULL
            AND tx_res_ack_sent_at IS NULL
            ORDER BY tx_exec_receipt_uploaded_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiFeeEntity>(sql)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
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
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                raw_tx = $3,
                tx_hash = $2,
                transaction_fee = $4,
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
    /// - 确保 scanner / recover 只基于有效事实工作
    ///
    /// 本方法是"事实回滚"，不是状态流转。
    /// 该方法不是重试控制，而是事实作废。
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许对尚未广播的交易调用（transaction_time IS NULL）
    /// - status 仅用于错误标注，不得用于流程推进
    /// - 📌 必须检查返回值 rows_affected()：
    ///   * rows_affected() == 0：表示事实已变更，无需处理
    ///   * rows_affected() == 1：表示成功作废事实
    ///   * 不建议直接忽略返回值
    pub async fn invalidate_raw_tx<'a, E>(
        exec: E,
        trade_no: &str,
        status: Option<ApiFeeStatus>,
        err_code: Option<u32>,
        err_msg: Option<&str>,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                raw_tx = NULL,
                tx_hash = NULL,
                status = COALESCE($2, status),
                err_code = COALESCE($3, err_code),
                err_msg = COALESCE($4, err_msg),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND transaction_time IS NULL
              AND raw_tx IS NOT NULL
        "#;

        let mut query = sqlx::query(sql).bind(trade_no);
        query = query.bind(status);
        query = query.bind(err_code);
        query = query.bind(err_msg);

        let res = query.execute(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// ⚠️ OBSERVATION ONLY
    /// This field is NOT used for:
    /// - concurrency control
    /// - execution decision
    /// - scanner logic
    /// Scanner MUST NOT depend on this field
    ///
    /// 更新building_at时间
    pub async fn update_building_at<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
            SET
                building_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND (building_at IS NULL OR building_at < datetime('now', '-30 seconds'))
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// ⚠️ OBSERVATION ONLY
    /// This field is NOT used for:
    /// - concurrency control
    /// - execution decision
    /// - scanner logic
    /// Scanner MUST NOT depend on this field
    ///
    /// 更新last_broadcast_at时间
    pub async fn update_last_broadcast_at<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
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
            UPDATE api_fee
            SET
                last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
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

    /// 标记 MQTT TxRes 已接收（外部事实）
    ///
    /// 语义：
    /// - 记录业务结果已就绪（来自 MQTT）
    /// - 只写入一次（幂等）
    /// - 不推进链、不修改状态
    ///
    /// ⚠️ 设计约束：
    /// - 禁止写 finished_at
    /// - 禁止修改 status
    /// - Scanner 只在 ResultAck 阶段读取该字段
    pub async fn update_tx_res_received_at<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
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
    /// | Recovery chain check | ❌                     | Use confirm_onchain_transaction_fact_with_recover |
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
            UPDATE api_fee
            SET
                tx_hash = $2,
                transaction_time = $3,
                transaction_fee = $4,
                resource_consume = $5,
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
            UPDATE api_fee
            SET
                last_broadcast_at = COALESCE(last_broadcast_at, $3),
                tx_hash = $2,
                transaction_time = $4,
                transaction_fee = $5,
                resource_consume = $6,
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
            UPDATE api_fee
            SET
                transaction_time = $2,
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

    /// 更新状态字段
    ///
    /// ⚠️ 仅由 recompute_and_update_status 调用
    /// ⚠️ 状态是派生字段，不是事实
    /// ⚠️ 不影响执行逻辑，仅用于显示
    pub async fn update_status<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiFeeStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_fee
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
}
