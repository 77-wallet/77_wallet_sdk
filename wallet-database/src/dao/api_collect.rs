use crate::{
    DbPool,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    pagination::Pagination,
};
use chrono::SecondsFormat;
use sqlx::{Executor, Row, Sqlite};

pub(crate) struct ApiCollectDao;

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

        let mut rec = sqlx::query_as::<_, ApiCollectEntity>(sql)
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
    /// ⚠️ 由于历史表结构限制，tx_hash / transaction_fee 在此写入哑值
    /// ⚠️ 未来表结构调整后应移除这些字段的绑定
    pub async fn add<'a, E>(exec: E, api_collect: ApiCollectEntity) -> Result<(), crate::Error>
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
                transaction_fee,
                created_at,
                updated_at,
                result_ack_send_count)
            VALUES
                ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT(trade_no) DO UPDATE SET
                updated_at          = strftime('%Y-%m-%dT%H:%M:%SZ','now')
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
            .bind(&api_collect.transaction_fee)
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

        let nonce = sqlx::query_scalar::<_, i32>(sql)
            .bind(from_addr)
            .bind(chain_code)
            .bind(nonce)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        tx.commit().await.map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
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
        err_code: u32,
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
        let sql = r#"
            UPDATE api_collect
            SET
                tx_hash = $2,
                transaction_time = $3,
                transaction_fee = $4,
                resource_consume = $5,
                finished_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND transaction_time IS NULL
              AND finished_at IS NULL
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

    /// ⚠️ Legacy: 状态机时代的遗留方法，使用status作为决策条件
    /// ⚠️ 未来应该移除，改用事实驱动的状态更新
    /// ⚠️ 禁止Scanner/Executor使用此方法
    pub async fn update_next_status<'a, E>(
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

    /// ⚠️ Legacy: 状态机时代的遗留方法，使用status作为决策条件
    /// ⚠️ 未来应该移除，改用事实驱动的状态更新
    /// ⚠️ 禁止Scanner/Executor使用此方法
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
              AND finished_at IS NULL
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
            UPDATE api_collect
            SET
                raw_tx = $3,
                tx_hash = $2,
                transaction_fee = $4,
                building_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND raw_tx IS NULL
              AND build_blocked_at IS NULL
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

    pub async fn set_order_ack_sent<'a, E>(exec: E, trade_no: &str) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND order_ack_sent_at IS NULL
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

    /// 扫描可构建的交易：raw_tx为空
    ///
    /// ⚠️ 核心事实驱动原则：
    /// - 只基于不可逆事实字段(raw_tx)决策
    /// - 不依赖时间字段(building_at)进行决策
    /// - 并发通过raw_tx写入唯一性保证
    pub async fn scan_can_build<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_collect 
            WHERE raw_tx IS NULL 
            AND build_blocked_at IS NULL
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
            AND transaction_time IS NULL 
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
    /// - transaction_time IS NOT NULL：链上已给出结果
    /// - finished_at IS NULL：系统生命周期未结束
    /// - result_ack_sent_at IS NULL：尚未发送结果确认（推进事实）
    ///
    /// ⚠️ 注意：
    /// - 不检查 result_ack_attempted_at（这是行为事实，不参与 Scanner 判断）
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
            AND finished_at IS NULL
            AND result_ack_sent_at IS NULL
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
            UPDATE api_collect
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
    /// - 并写入 build_blocked_at 事实
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
    pub async fn invalidate_raw_tx<'a, E>(
        exec: E,
        trade_no: &str,
        status: Option<ApiCollectStatus>,
        err_code: Option<u32>,
        err_msg: Option<&str>,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                raw_tx = NULL,
                tx_hash = NULL,
                build_blocked_at = COALESCE(
                    build_blocked_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                status = COALESCE($2, status),
                err_code = COALESCE($3, err_code),
                err_msg = COALESCE($4, err_msg),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND transaction_time IS NULL
              AND raw_tx IS NOT NULL
              AND build_blocked_at IS NULL
        "#;

        let mut query = sqlx::query(sql).bind(trade_no);
        query = query.bind(status);
        query = query.bind(err_code);
        query = query.bind(err_msg);

        let res = query.execute(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 清除构建阻断标记
    ///
    /// ⚠️ 设计约束：
    /// - 仅允许在“外部事实已发生”的前提下调用（如 fee 到账）
    /// - 本方法不会构建 raw_tx，只是解除构建阻断
    /// - 语义是：解除“不可构建”的事实，允许重新构建
    ///
    /// ⚠️ 调用约定：
    /// - 必须由产生新事实的一方调用（如 fee mqtt 处理器）
    /// - 禁止在 scanner / worker / retry 逻辑中调用
    pub async fn clear_build_blocked<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                build_blocked_at = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND build_blocked_at IS NOT NULL
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 标记 Result ACK 尝试（行为事实）
    ///
    /// 语义：
    /// - 只记录第一次尝试时间（COALESCE 幂等写）
    /// - confirmed 之后不再变化（WHERE result_ack_sent_at IS NULL）
    /// - 这是"行为事实"，不是"推进事实"
    /// - send_count 记录"尝试次数"，attempted_at 仅记录"首次尝试时间"
    /// - 二者语义不同，不得互相替代
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
                result_ack_attempted_at = COALESCE(
                    result_ack_attempted_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
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
    /// - 只能在 attempted 之后调用（WHERE result_ack_attempted_at IS NOT NULL）
    /// - 防止重复确认（WHERE result_ack_sent_at IS NULL）
    /// - 设置终态 finished_at
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
                result_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                finished_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND result_ack_attempted_at IS NOT NULL
              AND result_ack_sent_at IS NULL
              AND finished_at IS NULL
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
    /// 标记Result ACK发送，并设置终态
    pub async fn mark_result_ack_sent<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_collect
            SET
                result_ack_send_count = result_ack_send_count + 1,
                result_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                finished_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND finished_at IS NULL
              AND result_ack_sent_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }
}
