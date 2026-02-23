use crate::{
    DbPool,
    entities::{
        api_trade_type::ApiTradeType,
        api_withdraw::{
            ApiWithdrawEntity, ApiWithdrawStatus, ErrCode, WithdrawCreatedFact,
            WithdrawFailureStage,
        },
    },
    pagination::Pagination,
};
use chrono::{DateTime, TimeZone as _, Utc};
use sqlx::{Executor, QueryBuilder, Row, Sqlite};

// ⚠️ finished_at 为链终态事实字段
// ⚠️ 除 mark_chain_finished / mark_tx_res_ack_sent_and_chain_finished 外，禁止任何 UPDATE 语句写入 finished_at
// ⚠️ 未来 code review 时，搜索 `finished_at =` 并拒绝除上述方法外的所有情况

pub(crate) struct ApiWithdrawDao;

impl ApiWithdrawDao {
    pub async fn all_api_withdraw<'a, E>(
        exec: E,
        uid: &str,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"SELECT * FROM api_withdraws where uid = ? AND trade_type = ?"#;
        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(uid)
            .bind(ApiTradeType::Withdraw)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    pub async fn list_api_withdraw_with_status<'a, E>(
        exec: E,
        vec_status: Vec<ApiWithdrawStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut qb = QueryBuilder::<Sqlite>::new("SELECT * FROM api_withdraws WHERE trade_type = ");
        qb.push_bind(ApiTradeType::Withdraw as u8);
        if !vec_status.is_empty() {
            qb.push(" AND status IN (");
            let mut separated = qb.separated(", "); // 自动在元素间加逗号
            for status in &vec_status {
                separated.push_bind(status);
            }
            qb.push(")");
        }

        qb.push(" ORDER BY updated_at ASC, created_at ASC");
        qb.push(" LIMIT ").push_bind(page_size);
        qb.push(" OFFSET ").push_bind(page * page_size);
        let query = qb.build_query_as::<ApiWithdrawEntity>();
        let rows = query.fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))?;
        Ok(rows)
    }

    pub async fn page_api_withdraw<'a, E>(
        pool: &E,
        uid: &str,
        vec_status: Vec<ApiWithdrawStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error>
    where
        for<'c> &'c E: sqlx::Executor<'c, Database = sqlx::Sqlite>,
    {
        let mut count_qb =
            QueryBuilder::<Sqlite>::new("SELECT count(*) FROM api_withdraws WHERE trade_type = ");
        count_qb.push_bind(ApiTradeType::Withdraw as u8);
        let mut qb = QueryBuilder::<Sqlite>::new("SELECT * FROM api_withdraws WHERE trade_type = ");
        qb.push_bind(ApiTradeType::Withdraw as u8);
        if !uid.is_empty() {
            count_qb.push(" AND uid = ").push_bind(uid);
            qb.push(" AND uid = ").push_bind(uid);
        }
        if !vec_status.is_empty() {
            count_qb.push(" AND status IN (");
            qb.push(" AND status IN (");
            let mut count_separated = count_qb.separated(", ");
            let mut separated = qb.separated(", "); // 自动在元素间加逗号
            for status in &vec_status {
                count_separated.push_bind(status);
                separated.push_bind(status);
            }
            count_qb.push(")");
            qb.push(")");
        }
        let count_query = count_qb.build_query_scalar();
        let total_count =
            count_query.fetch_one(pool).await.map_err(|e| crate::Error::Database(e.into()))?;

        qb.push(" ORDER BY updated_at DESC, created_at DESC");
        qb.push(" LIMIT ").push_bind(page_size);
        qb.push(" OFFSET ").push_bind(page * page_size);
        let query = qb.build_query_as::<ApiWithdrawEntity>();
        let rows = query.fetch_all(pool).await.map_err(|e| crate::Error::Database(e.into()))?;

        let mut paginate = Pagination::<ApiWithdrawEntity>::init(page, page_size);
        paginate.data = rows;
        paginate.total_count = total_count;
        Ok(paginate)
    }

    pub async fn page_api_withdraw_with_status<'a, E>(
        exec: E,
        _page: i64,
        page_size: i64,
        vec_status: &[ApiWithdrawStatus],
    ) -> Result<(i64, Vec<ApiWithdrawEntity>), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Clone,
    {
        let placeholders = vec_status.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let count_sql = format!(
            "SELECT count(*) FROM api_withdraws where status in ({})
",
            placeholders
        );
        let sql = format!(
            "SELECT * FROM api_withdraws where status in ({}) ORDER BY id ASC LIMIT ?",
            placeholders
        );

        let mut query = sqlx::query_scalar::<_, i64>(&count_sql);
        for status in vec_status {
            query = query.bind(status);
        }
        let count =
            query.fetch_one(exec.clone()).await.map_err(|e| crate::Error::Database(e.into()))?;

        let mut query = sqlx::query_as::<_, ApiWithdrawEntity>(&sql);
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

    pub async fn page_api_withdraw_with_init_status<'a, E>(
        pool: &E,
        uid: &str,
        init_status: ApiWithdrawStatus,
        vec_status: Vec<ApiWithdrawStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error>
    where
        for<'c> &'c E: sqlx::Executor<'c, Database = sqlx::Sqlite>,
    {
        let mut count_qb =
            QueryBuilder::<Sqlite>::new("SELECT count(*) FROM api_withdraws WHERE trade_type = ");
        let mut qb = QueryBuilder::<Sqlite>::new("SELECT * FROM api_withdraws WHERE trade_type = ");
        count_qb.push_bind(ApiTradeType::Withdraw);
        count_qb.push(" AND init_status = ").push_bind(init_status);
        qb.push_bind(ApiTradeType::Withdraw);
        qb.push(" AND init_status = ").push_bind(init_status);
        if !uid.is_empty() {
            count_qb.push(" AND uid = ").push_bind(uid);
            qb.push(" AND uid = ").push_bind(uid);
        }
        if !vec_status.is_empty() {
            if vec_status.len() == 1 {
                count_qb.push(" AND status = ").push_bind(&vec_status[0]);
                qb.push(" AND status =  ").push_bind(&vec_status[0]);
            } else {
                count_qb.push(" AND status IN (");
                qb.push(" AND status IN (");
                let mut count_separated = count_qb.separated(", ");
                let mut separated = qb.separated(", "); // 自动在元素间加逗号
                for status in &vec_status {
                    count_separated.push_bind(status);
                    separated.push_bind(status);
                }
                count_qb.push(")");
                qb.push(")");
            }
        }

        let count_query = count_qb.build_query_scalar();
        let total_count =
            count_query.fetch_one(pool).await.map_err(|e| crate::Error::Database(e.into()))?;

        qb.push(" ORDER BY updated_at DESC, created_at DESC");
        qb.push(" LIMIT ").push_bind(page_size);
        qb.push(" OFFSET ").push_bind(page * page_size);
        let query = qb.build_query_as::<ApiWithdrawEntity>();
        let rows = query.fetch_all(pool).await.map_err(|e| crate::Error::Database(e.into()))?;

        let mut paginate = Pagination::<ApiWithdrawEntity>::init(page, page_size);
        paginate.total_count = total_count;
        paginate.data = rows;
        Ok(paginate)
    }

    pub async fn get_api_withdraw_by_id<'a, E>(
        exec: E,
        id: &str,
    ) -> Result<ApiWithdrawEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_withdraws WHERE id = ?";
        let res = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(id)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    pub async fn get_api_withdraw_by_trade_no<'a, E>(
        exec: E,
        trade_no: &str,
        trade_type: ApiTradeType,
    ) -> Result<ApiWithdrawEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_withdraws WHERE trade_no = ? AND trade_type = ?";
        let res = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(trade_no)
            .bind(trade_type)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    pub async fn get_api_withdraw_by_trade_no_status<'a, E>(
        exec: E,
        trade_no: &str,
        vec_status: &[ApiWithdrawStatus],
    ) -> Result<ApiWithdrawEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let placeholders = vec_status.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT * FROM api_withdraws where trade_no = ? AND trade_type = ? AND status in ({})",
            placeholders
        );
        let mut query = sqlx::query_as::<_, ApiWithdrawEntity>(&sql)
            .bind(trade_no)
            .bind(ApiTradeType::Withdraw);
        for status in vec_status {
            query = query.bind(status);
        }
        let res = query.fetch_one(exec).await.map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    pub async fn get_by_hash_and_owner<'a, E>(
        exec: E,
        owner: &str,
        tx_hash: &str,
    ) -> Result<ApiWithdrawEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_withdraws WHERE from_addr = ? AND tx_hash = ?";
        let res = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(owner)
            .bind(tx_hash)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    pub async fn lists_by_hashs<'a, E>(
        exec: E,
        owner: &str,
        hashs: Vec<String>,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut qb =
            QueryBuilder::<Sqlite>::new("SELECT * FROM api_withdraws WHERE `from_addr` = ");

        qb.push_bind(owner);
        qb.push(" AND trade_type = ").push_bind(ApiTradeType::SelfWithdraw);
        // 绑定多个 hash
        qb.push(" AND tx_hash IN (");
        qb.push_values(hashs.iter(), |mut b, h| {
            b.push_bind(h);
        });
        qb.push(")");

        let query = qb.build_query_as::<ApiWithdrawEntity>();

        let res = query.fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    pub async fn recent_bill<'a, E>(
        exec: &E,
        token: &str,
        from_addr: &str,
        chain_code: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error>
    where
        for<'c> &'c E: sqlx::Executor<'c, Database = sqlx::Sqlite>,
    {
        let mut count_qb =
            QueryBuilder::<Sqlite>::new("SELECT count(*) FROM api_withdraws WHERE trade_type=4 ");
        let mut qb = QueryBuilder::<Sqlite>::new("SELECT * FROM api_withdraws WHERE trade_type=4 ");
        if !from_addr.is_empty() {
            count_qb.push("AND from_addr = ").push_bind(from_addr);
            qb.push("AND from_addr = ").push_bind(from_addr);
        }
        if !chain_code.is_empty() {
            count_qb.push("AND chain_code = ").push_bind(chain_code);
            qb.push("AND chain_code = ").push_bind(chain_code);
        }
        if !token.is_empty() {
            count_qb.push("AND token_addr = ").push_bind(token);
            qb.push("AND token_addr = ").push_bind(token);
        }

        // count
        count_qb.push(" GROUP BY to_addr");
        let count_query = count_qb.build_query_scalar();
        let total_count =
            count_query.fetch_one(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

        // list
        qb.push(" GROUP BY to_addr");
        qb.push(" ORDER BY updated_at DESC, created_at DESC");
        qb.push(" LIMIT ").push_bind(page_size);
        qb.push(" OFFSET ").push_bind(page * page_size);
        let query = qb.build_query_as::<ApiWithdrawEntity>();
        let rows = query.fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

        let mut paginate = Pagination::<ApiWithdrawEntity>::init(page, page_size);
        paginate.total_count = total_count;
        paginate.data = rows;
        Ok(paginate)
    }

    pub async fn bill_lists<'a, E>(
        exec: &E,
        uid: &str,
        addr: &[String],
        chain_code: Option<&str>,
        symbol: Option<&str>,
        is_multisig: Option<i64>,
        min_value: Option<f64>,
        start: Option<i64>,
        end: Option<i64>,
        transfer_type: Vec<i32>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error>
    where
        for<'c> &'c E: sqlx::Executor<'c, Database = sqlx::Sqlite>,
    {
        let (mut count_qb, mut qb) = if transfer_type.len() > 0 {
            let count_qb_s = "SELECT count(*) FROM api_withdraws WHERE ".to_string();
            let qb_s = "SELECT * FROM api_withdraws WHERE ".to_string();
            let mut conds: Vec<&str> = vec![];
            for tt in transfer_type {
                if tt == ApiTradeType::Withdraw as i32 {
                    conds.push("(trade_type = 1 AND init_status = 0 AND status in (3,5,7,8,9,10))");
                } else if tt == ApiTradeType::SelfWithdraw as i32 {
                    conds.push("trade_type = 4");
                } else if tt == ApiTradeType::SelfRecharge as i32 {
                    conds.push("trade_type = 5");
                }
            }
            let s = conds.join(" OR ");
            (
                QueryBuilder::<Sqlite>::new(count_qb_s + " ( " + s.as_str() + " ) "),
                QueryBuilder::<Sqlite>::new(qb_s + " ( " + s.as_str() + " ) "),
            )
        } else {
            (
                QueryBuilder::<Sqlite>::new(
                    "SELECT count(*) FROM api_withdraws WHERE ((trade_type = 1 AND init_status = 0 AND status in (3,5,7,8,9,10)) OR trade_type IN (4,5)) ",
                ),
                QueryBuilder::<Sqlite>::new(
                    "SELECT * FROM api_withdraws WHERE ((trade_type = 1 AND init_status = 0 AND status in (3,5,7,8,9,10)) OR trade_type IN (4,5)) ",
                ),
            )
        };
        if !uid.is_empty() {
            count_qb.push(" AND uid = ").push_bind(uid);
            qb.push(" AND uid = ").push_bind(uid);
        }
        if let Some(c) = chain_code {
            // tracing::info!("chain code: {}", c);
            count_qb.push(" AND chain_code = ").push_bind(c);
            qb.push(" AND chain_code = ").push_bind(c);
        }
        if let Some(c) = symbol {
            count_qb.push(" AND symbol = ").push_bind(c);
            qb.push(" AND symbol = ").push_bind(c);
        }
        if let Some(c) = min_value {
            count_qb.push(" AND CAST(value AS REAL) >= ").push_bind(c);
            qb.push(" AND CAST(value AS REAL) >= ").push_bind(c);
        }
        if let Some(c) = start {
            let dt: DateTime<Utc> = Utc.timestamp(c, 0);
            tracing::info!(" ==== start {:?}", dt);
            count_qb.push(" AND transaction_time >= ").push_bind(dt);
            qb.push(" AND transaction_time >= ").push_bind(dt);
        }
        if let Some(c) = end {
            let dt: DateTime<Utc> = Utc.timestamp(c, 0);
            tracing::info!(" ==== end {:?}", dt);
            count_qb.push(" AND transaction_time <= ").push_bind(dt);
            qb.push(" AND transaction_time <= ").push_bind(dt);
        }

        tracing::info!("query_count={:?}", count_qb.sql());
        let count_query = count_qb.build_query_scalar();
        let total_count =
            count_query.fetch_one(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

        qb.push(" ORDER BY updated_at DESC, created_at DESC");
        qb.push(" LIMIT ").push_bind(page_size);
        qb.push(" OFFSET ").push_bind(page * page_size);
        let query = qb.build_query_as::<ApiWithdrawEntity>();
        let rows = query.fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

        let mut paginate = Pagination::<ApiWithdrawEntity>::init(page, page_size);
        paginate.total_count = total_count;
        paginate.data = rows;
        Ok(paginate)
    }

    pub async fn upsert<'a, E>(exec: E, api_withdraw: ApiWithdrawEntity) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO api_withdraws
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
                init_status,
                status,
                tx_hash,
                resource_consume,
                transaction_fee,
                transaction_time,
                block_height,
                notes,
                created_at,
                updated_at)
            VALUES
                ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,
                 $11,$12,$13,$14,$15,$16,$17,$18,$19,
                 strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 strftime('%Y-%m-%dT%H:%M:%SZ','now'))
            ON CONFLICT(trade_no) DO UPDATE SET
                updated_at          = strftime('%Y-%m-%dT%H:%M:%SZ','now')
        "#;

        let res = sqlx::query(sql)
            .bind(&api_withdraw.uid)
            .bind(&api_withdraw.name)
            .bind(&api_withdraw.from_addr)
            .bind(&api_withdraw.to_addr)
            .bind(&api_withdraw.value)
            .bind(&api_withdraw.validate)
            .bind(&api_withdraw.chain_code)
            .bind(&api_withdraw.token_addr)
            .bind(&api_withdraw.symbol)
            .bind(&api_withdraw.trade_no)
            .bind(&api_withdraw.trade_type)
            .bind(&api_withdraw.init_status)
            .bind(&api_withdraw.status)
            .bind(&api_withdraw.tx_hash) // hash
            .bind(&api_withdraw.resource_consume) // consume
            .bind(&api_withdraw.transaction_fee) // fee
            .bind(api_withdraw.transaction_time) // time
            .bind(api_withdraw.block_height) // block height
            .bind(&api_withdraw.notes)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        tracing::info!(rows_affected=%res.rows_affected(), "withdraw api");
        Ok(())
    }

    pub async fn add<'a, E>(exec: E, api_withdraw: WithdrawCreatedFact) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO api_withdraws
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
            .bind(&api_withdraw.uid)
            .bind(&api_withdraw.name)
            .bind(&api_withdraw.from_addr)
            .bind(&api_withdraw.to_addr)
            .bind(&api_withdraw.value)
            .bind(&api_withdraw.validate)
            .bind(&api_withdraw.chain_code)
            .bind(api_withdraw.token_addr)
            .bind(&api_withdraw.symbol)
            .bind(&api_withdraw.trade_no)
            .bind(api_withdraw.trade_type)
            .bind(api_withdraw.status)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        tracing::info!(xx=%res.rows_affected(), "tx withdraw api");
        Ok(())
    }

    pub async fn update_status_and_err<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiWithdrawStatus,
        err_code: ErrCode,
        err_msg: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                status = $2,
                err_code = $3,
                err_msg = $4,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND err_code IS NULL
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
        status: ApiWithdrawStatus,
        next_status: ApiWithdrawStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                status = $3,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1 and status = $2
              AND err_code IS NULL
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

    /// 更新交易状态和 nonce（含链级 nonce 管理）
    ///
    /// nonce 语义说明：
    /// - api_nonce.nonce 是 single source of truth（链级）
    /// - api_withdraw.nonce 是「本次交易使用的 nonce 快照」
    /// - api_withdraw.nonce 只用于审计 / 追溯，不参与 nonce 计算
    ///
    /// 约束：
    /// - 任何 nonce 计算必须基于 api_nonce
    /// - 禁止从 api_withdraw.nonce 反推下一个 nonce
    /// - 禁止在 api_withdraw 中对 nonce 进行自增操作
    pub async fn update_tx_status_nonce(
        pool: &DbPool,
        from_addr: &str,
        chain_code: &str,
        trade_no: &str,
        nonce: i64,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiWithdrawStatus,
    ) -> Result<u64, crate::Error> {
        let mut tx = pool.begin().await.map_err(|e| crate::Error::Database(e.into()))?;
        let sql = r#"
            UPDATE api_withdraws
            SET
                tx_hash = $2,
                nonce = $3,
                resource_consume = $4,
                transaction_fee = $5,
                status = $6,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
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
                nonce = excluded.nonce,
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
    }

    pub async fn update_tx_status<'a, E>(
        exec: E,
        trade_no: &str,
        nonce: i64,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        block_height: &str,
        status: ApiWithdrawStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                status = $2,
                nonce = $3,
                tx_hash = $4,
                resource_consume = $5,
                transaction_fee = $6,
                transaction_time = $7,
                block_height = $8,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(status)
            .bind(nonce)
            .bind(tx_hash)
            .bind(resource_consume)
            .bind(transaction_fee)
            .bind(transaction_time)
            .bind(block_height)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    pub async fn update_tx<'a, E>(
        exec: E,
        trade_no: &str,
        resource_consume: &str,
        transaction_fee: &str,
        transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        block_height: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                resource_consume = $2,
                transaction_fee = $3,
                transaction_time = $4,
                block_height = $5,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;

        sqlx::query(sql)
            .bind(trade_no)
            .bind(resource_consume)
            .bind(transaction_fee)
            .bind(transaction_time)
            .bind(block_height)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn update_post_tx_count<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
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
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
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
            FROM api_withdraws
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
            UPDATE api_withdraws
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
            UPDATE api_withdraws
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
            UPDATE api_withdraws
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
            UPDATE api_withdraws
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
            UPDATE api_withdraws
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
            UPDATE api_withdraws
            SET
                tx_res_ack_sent_at = COALESCE(
                    tx_res_ack_sent_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
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

    /// 原子标记链上终态（唯一合法 finished_at 写入口）
    ///
    /// 语义：
    /// - 这是"广播成功 → 链上确认"的不可逆事实跃迁
    /// - 单条 SQL 原子更新，防止 kill -9 产生"半完成事实"
    /// - WHERE 带旧事实约束，保证并发安全
    ///
    /// 写入顺序约束（不可逆）：
    /// raw_tx → tx_hash → transaction_time → finished_at
    pub async fn mark_chain_finished<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
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

    /// 原子标记交易结果 ACK 已发送并标记链上终态
    ///
    /// 语义：
    /// - 交易结果 ACK 已成功发送到后端
    /// - 同时标记链上终态
    /// - 单条 SQL 原子更新，防止 kill -9 产生"半完成事实"
    /// - WHERE 带旧事实约束，保证并发安全
    ///
    /// 写入顺序约束（不可逆）：
    /// raw_tx → tx_hash → transaction_time → finished_at
    pub async fn mark_tx_res_ack_sent_and_chain_finished<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                tx_res_ack_sent_at = COALESCE(
                    tx_res_ack_sent_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                tx_res_ack_attempted_at = COALESCE(
                    tx_res_ack_attempted_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                ),
                finished_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND tx_res_ack_sent_at IS NULL
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
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_withdraws 
            WHERE tx_ack_sent_at IS NULL
            AND finished_at IS NULL
            AND err_code IS NULL
            AND trade_type = ?
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(ApiTradeType::Withdraw)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 扫描需要恢复交易的记录
    ///
    /// 事实条件：
    /// - tx_hash IS NOT NULL：交易已构建并广播
    /// - transaction_time IS NULL：链上结果未确认
    /// - last_broadcast_at IS NULL：广播行为尚未记录
    /// - finished_at IS NULL：系统生命周期未结束
    /// - err_code IS NULL：无终止错误
    ///
    /// ⚠️ 重要约束：
    /// - SQL必须100%等价于scanner中的need_recover predicate
    ///
    /// ============================================================================
    /// ⚠️ SAFETY GATE (DO NOT REMOVE):
    ///
    /// last_broadcast_at IS NULL is intentionally required
    /// because:
    ///
    /// - Recover performs on-chain RPC queries
    /// - System currently has NO cooldown / backoff / last_recover_at
    /// - Removing this gate will cause infinite recover loops
    ///   and RPC storm after restart
    ///
    /// This gate may ONLY be removed if:
    /// - last_recover_at OR cooldown mechanism is introduced
    /// ============================================================================
    pub async fn scan_need_recover<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_withdraws 
            WHERE tx_hash IS NOT NULL
            AND transaction_time IS NULL
            AND last_broadcast_at IS NULL
            AND finished_at IS NULL
            AND err_code IS NULL
            AND trade_type = ?
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(ApiTradeType::Withdraw)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    // ============================================================================
    // STRONG ORDER GATE — BuildTx 不可逆事实屏障
    // ============================================================================
    //
    // BuildTx 只能在以下事实全部满足时发生：
    //
    // [FACT REQUIRED]
    // ✔ tx_ack_sent_at IS NOT NULL   — 后端确认已发送
    // ✔ audit_passed_at IS NOT NULL  — 审计通过（强顺序屏障）
    //
    // [FACT MUST NOT EXIST]
    // ✘ raw_tx IS NOT NULL           — 防止重复构建
    // ✘ finished_at IS NOT NULL      — 已终态
    // ✘ err_code IS NOT NULL         — 终止错误
    //
    // ⚠️ DO NOT REMOVE ANY CONDITION
    // ⚠️ Scanner 与 DAO predicate 必须完全一致
    // ============================================================================
    // ============================================================================
    // ⚠️ Scanner / DAO Predicate Symmetry Rule
    //
    // 本方法 SQL predicate 必须与：
    // wallet_api::infrastructure::withdraw::shadow::scanner::can_build 完全一致
    //
    // 修改任一侧时必须同步修改另一侧，否则会导致：
    // - Phantom Task
    // - Double Build
    // - 永久卡死
    //
    // DAO 是事实来源，Scanner 是安全网
    // ============================================================================
    pub async fn scan_can_build<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            -- ============================================================================
            -- STRONG ORDER GATE — BuildTx 不可逆事实屏障
            -- ============================================================================
            --
            -- BuildTx 只能在以下事实全部满足时发生：
            --
            -- [FACT REQUIRED]
            -- ✔ tx_ack_sent_at IS NOT NULL   — 后端确认已发送
            -- ✔ audit_passed_at IS NOT NULL  — 审计通过（强顺序屏障）
            --
            -- [FACT MUST NOT EXIST]
            -- ✘ raw_tx IS NOT NULL           — 防止重复构建
            -- ✘ finished_at IS NOT NULL      — 已终态
            -- ✘ err_code IS NOT NULL         — 终止错误
            --
            -- ⚠️ DO NOT REMOVE ANY CONDITION
            -- ⚠️ Scanner 与 DAO predicate 必须完全一致
            -- ============================================================================
            SELECT * FROM api_withdraws 
            WHERE tx_ack_sent_at IS NOT NULL
            AND audit_passed_at IS NOT NULL
            AND raw_tx IS NULL 
            AND finished_at IS NULL
            AND err_code IS NULL
            AND trade_type = ?
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(ApiTradeType::Withdraw)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 扫描可广播的交易
    ///
    /// 事实条件：
    /// - raw_tx IS NOT NULL：交易已构建
    /// - last_broadcast_at IS NULL：广播行为尚未发生
    /// - finished_at IS NULL：系统生命周期未结束
    /// - err_code IS NULL：无终止错误
    /// - tx_ack_sent_at IS NOT NULL：后端确认已发送
    ///
    /// ⚠️ 核心事实驱动原则：
    /// - 只基于不可逆事实字段决策
    /// - 并发通过transaction_time写入唯一性保证
    ///
    /// ============================================================================
    /// Broadcast model:
    ///
    /// - last_broadcast_at is written ONLY after successful submission
    /// - Failed attempts DO NOT write broadcast fact
    /// - Scanner relies on this to allow retry
    ///
    /// DO NOT remove last_broadcast_at gate unless
    /// retry backoff or cooldown is implemented.
    /// ============================================================================
    pub async fn scan_can_broadcast<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_withdraws 
            WHERE raw_tx IS NOT NULL 
            AND last_broadcast_at IS NULL 
            AND finished_at IS NULL
            AND err_code IS NULL
            AND tx_ack_sent_at IS NOT NULL
            AND trade_type = ?
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(ApiTradeType::Withdraw)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 扫描需要上传交易执行回执的交易
    ///
    /// 事实条件：
    /// - tx_exec_receipt_uploaded_at IS NULL：尚未上传执行回执
    /// - finished_at IS NULL：系统生命周期未结束
    /// - (last_broadcast_at IS NOT NULL OR err_code IS NOT NULL)：
    ///     - 已发生 Broadcast 行为（节点已接受交易提交）
    ///     - 或出现终止型错误
    ///
    /// ⚠️ 架构铁律：
    /// - UploadTxExecReceipt =【执行行为回执】
    /// - 表示系统已执行 SendRawTx 并收到节点响应
    /// - 不代表链确认
    /// - 不依赖 transaction_time
    /// - tx_hash 只是构建事实，不能作为执行回执 gate
    ///
    /// ⚠️ err_code 仍允许上传：
    /// - 属于行为事实补齐副作用
    /// - 不属于推进，不受 err_code 冻结
    ///
    /// ⚠️ scanner 冻结（等待 tx_hash 补齐）：
    /// - 若 withdraw 会构造 Success 回执（且非 chain_failed / err_code 失败路径），但 tx_hash 缺失
    /// - 则本地已知该回执无法成功上传，scanner 不应重复投递
    /// - 待后续事实补齐 tx_hash 后会自动重新进入扫描结果（无需显式解冻）
    pub async fn scan_need_tx_exec_receipt_upload<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_withdraws 
            WHERE finished_at IS NULL
            AND tx_exec_receipt_uploaded_at IS NULL
            AND trade_type = ?
            AND (
                last_broadcast_at IS NOT NULL
                OR err_code IS NOT NULL
                OR transaction_time IS NOT NULL
            )
            AND NOT (
                err_code IS NULL
                AND chain_failed_at IS NULL
                AND (
                    chain_success_at IS NOT NULL
                    OR transaction_time IS NOT NULL
                    OR last_broadcast_at IS NOT NULL
                )
                AND (
                    tx_hash IS NULL
                    OR trim(tx_hash) = ''
                )
            )
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(ApiTradeType::Withdraw)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 扫描需要发送交易结果 ACK 的交易
    ///
    /// 事实条件：
    /// - tx_exec_receipt_uploaded_at IS NOT NULL：交易执行回执已上传
    /// - finished_at IS NULL：系统生命周期未结束
    /// - transaction_time IS NOT NULL：链上结果已确认
    /// - tx_res_ack_sent_at IS NULL：尚未发送交易结果 ACK（推进事实）
    /// - err_code IS NULL：无终止错误
    ///
    /// ⚠️ 强顺序屏障：
    /// - TxResAck 必须发生在 TxExecReceipt 上传之后
    /// - TxResAck 必须发生在链上结果确认之后
    /// - 使用 transaction_time 作为前置条件是必要的，确保只对已确认的交易发送 ACK
    ///
    /// ⚠️ 注意：
    /// - 不检查 tx_res_ack_attempted_at（这是行为事实，不参与 Scanner 判断）
    /// - attempted 只用于 Worker / 运维观测
    pub async fn scan_need_tx_res_ack<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            -- ⚠️ 强顺序屏障：
            -- TxResAck 必须发生在 TxExecReceipt 上传之后
            -- TxResAck 必须发生在链上结果确认之后
            -- 使用 transaction_time 作为前置条件是必要的，确保只对已确认的交易发送 ACK
            SELECT * FROM api_withdraws 
            WHERE tx_exec_receipt_uploaded_at IS NOT NULL
            AND finished_at IS NULL
            AND transaction_time IS NOT NULL
            AND tx_res_received_at IS NOT NULL
            AND tx_res_ack_sent_at IS NULL
            AND err_code IS NULL
            AND trade_type = ?
            ORDER BY tx_exec_receipt_uploaded_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(ApiTradeType::Withdraw)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 周期性卡单预筛选：扫描“可能卡住”的交易（低成本）
    ///
    /// 设计目标：
    /// - 尽可能便宜：只做粗筛
    /// - 宁可多报：后续由 wallet-api 的 DiagnoseEngine 二次判断
    ///
    /// 粗筛条件：
    /// - finished_at IS NULL：尚未终态
    /// - created_at < now - 5 minutes：避免刚创建的正常单刷屏
    /// - 已有进展事实（任意一个存在）：
    ///   - tx_ack_sent_at / raw_tx / last_broadcast_at / transaction_time
    ///
    /// ⚠️ 注意：
    /// - 不排除 err_code：失败冻结态仍可能需要补齐 receipt 等行为事实
    pub async fn scan_possible_stuck<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_withdraws
            WHERE finished_at IS NULL
              AND trade_type = ?
              AND created_at < strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-5 minutes')
              AND (
                tx_ack_sent_at IS NOT NULL
                OR raw_tx IS NOT NULL
                OR last_broadcast_at IS NOT NULL
                OR transaction_time IS NOT NULL
              )
            ORDER BY created_at ASC
            LIMIT ?
        "#;
        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(ApiTradeType::Withdraw)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    /// 更新构建完成后的交易信息，包括raw_tx、tx_hash、transaction_fee、nonce和building_at
    ///
    /// ⚠️ 写入顺序约束（不可逆）：
    /// raw_tx → tx_hash → transaction_time → finished_at
    /// - 不允许写tx_hash时raw_tx还是NULL
    /// - 不允许写transaction_time时tx_hash是NULL
    /// - 不允许写finished_at时transaction_time是NULL
    ///
    /// ⚠️ nonce 语义：
    /// - nonce 是在 phase 1 分配的已裁决事实
    /// - 一旦写入，不允许修改
    /// - recover_tx 依赖此值进行交易恢复
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
            UPDATE api_withdraws
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
        status: Option<ApiWithdrawStatus>,
        err_code: Option<u32>,
        err_msg: Option<&str>,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                raw_tx = NULL,
                tx_hash = NULL,
                building_at = NULL,
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
            UPDATE api_withdraws
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

    /// 更新last_broadcast_at时间
    pub async fn update_last_broadcast_at<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
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

    /// 标记广播已执行
    pub async fn mark_broadcast_executed<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND last_broadcast_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 确认链上交易事实
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
            UPDATE api_withdraws
            SET
                tx_hash = $2,
                transaction_time = $3,
                chain_success_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                chain_failed_at = NULL,
                resource_consume = $4,
                transaction_fee = $5,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND transaction_time IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(tx_hash)
            .bind(transaction_time)
            .bind(resource_consume)
            .bind(transaction_fee)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 确认链上交易事实（带恢复）
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
            UPDATE api_withdraws
            SET
                last_broadcast_at = COALESCE(last_broadcast_at, $3),
                tx_hash = $2,
                transaction_time = $4,
                chain_success_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                chain_failed_at = NULL,
                resource_consume = $5,
                transaction_fee = $6,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND transaction_time IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(tx_hash)
            .bind(last_broadcast_at)
            .bind(transaction_time)
            .bind(resource_consume)
            .bind(transaction_fee)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 确认交易时间（如果不存在）
    pub async fn confirm_transaction_time_if_absent<'a, E>(
        exec: E,
        trade_no: &str,
        transaction_time: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
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

    /// 设置审核通过事实
    pub async fn set_audit_passed<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                audit_passed_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                audit_rejected_at = NULL,
                audit_reason = NULL,
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

    /// 设置审核拒绝事实
    pub async fn set_audit_rejected<'a, E>(
        exec: E,
        trade_no: &str,
        reason: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                audit_rejected_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                audit_passed_at = NULL,
                audit_reason = $2,
                err_code = $3,
                err_msg = $4,
                finished_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(reason)
            .bind(ErrCode::UnknownError)
            .bind(reason)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 设置链成功事实
    pub async fn set_chain_success<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                chain_success_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                chain_failed_at = NULL,
                failure_stage = NULL,
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

    /// 设置链失败事实
    pub async fn set_chain_failed<'a, E>(exec: E, trade_no: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                chain_failed_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                chain_success_at = NULL,
                failure_stage = NULL,
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

    /// 设置失败阶段事实
    pub async fn set_failure_stage<'a, E>(
        exec: E,
        trade_no: &str,
        stage: WithdrawFailureStage,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                failure_stage = $2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
              AND chain_success_at IS NULL
              AND chain_failed_at IS NULL
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(stage)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// 更新状态
    pub async fn update_status<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                status = $2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;
        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(status)
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
            UPDATE api_withdraws
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
}

#[cfg(test)]
mod tests {
    use super::ApiWithdrawDao;
    use crate::{
        SqliteContext,
        entities::{api_trade_type::ApiTradeType, api_withdraw::ApiWithdrawStatus},
        repositories::api_wallet::withdraw::ApiWithdrawRepo,
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
    async fn scan_possible_stuck_prefilter_works() {
        let dir = make_temp_dir("wallet_db_api_withdraw_scan_possible_stuck");
        let ctx = SqliteContext::new(&dir, Some("api_funds.db")).await.unwrap();
        let pool = ctx.into_collect_db_pool().unwrap();

        // old + progressed => included
        ApiWithdrawRepo::upsert_api_withdraw(
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
            "W_STUCK_1",
            ApiTradeType::Withdraw,
            0,
            None,
            ApiWithdrawStatus::Init,
            ApiWithdrawStatus::Init,
            "0",
            "0",
            None,
            None,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_withdraws SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ','now','-10 minutes'), tx_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE trade_no = ?",
        )
        .bind("W_STUCK_1")
        .execute(pool.as_ref())
        .await
        .unwrap();

        // too new => excluded
        ApiWithdrawRepo::upsert_api_withdraw(
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
            "W_STUCK_2",
            ApiTradeType::Withdraw,
            0,
            None,
            ApiWithdrawStatus::Init,
            ApiWithdrawStatus::Init,
            "0",
            "0",
            None,
            None,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_withdraws SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ','now','-1 minutes'), tx_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE trade_no = ?",
        )
        .bind("W_STUCK_2")
        .execute(pool.as_ref())
        .await
        .unwrap();

        // finished => excluded
        ApiWithdrawRepo::upsert_api_withdraw(
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
            "W_STUCK_3",
            ApiTradeType::Withdraw,
            0,
            None,
            ApiWithdrawStatus::Init,
            ApiWithdrawStatus::Init,
            "0",
            "0",
            None,
            None,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_withdraws SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ','now','-10 minutes'), tx_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'), finished_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE trade_no = ?",
        )
        .bind("W_STUCK_3")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let rows = ApiWithdrawDao::scan_possible_stuck(pool.as_ref(), 100).await.unwrap();
        let trade_nos: std::collections::HashSet<_> =
            rows.into_iter().map(|r| r.trade_no).collect();

        assert!(trade_nos.contains("W_STUCK_1"));
        assert!(!trade_nos.contains("W_STUCK_2"));
        assert!(!trade_nos.contains("W_STUCK_3"));
    }

    #[tokio::test]
    async fn scan_need_tx_res_ack_requires_tx_res_received_at() {
        let dir = make_temp_dir("wallet_db_api_withdraw_scan_need_tx_res_ack_gate");
        let ctx = SqliteContext::new(&dir, Some("api_funds.db")).await.unwrap();
        let pool = ctx.into_collect_db_pool().unwrap();

        // record A: eligible
        ApiWithdrawRepo::upsert_api_withdraw(
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
            "W_TX_RES_A",
            ApiTradeType::Withdraw,
            0,
            None,
            ApiWithdrawStatus::Init,
            ApiWithdrawStatus::Init,
            "0",
            "0",
            None,
            None,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_withdraws SET tx_exec_receipt_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'), transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ','now'), tx_res_received_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE trade_no = ?",
        )
        .bind("W_TX_RES_A")
        .execute(pool.as_ref())
        .await
        .unwrap();

        // record B: missing tx_res_received_at => must be excluded
        ApiWithdrawRepo::upsert_api_withdraw(
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
            "W_TX_RES_B",
            ApiTradeType::Withdraw,
            0,
            None,
            ApiWithdrawStatus::Init,
            ApiWithdrawStatus::Init,
            "0",
            "0",
            None,
            None,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE api_withdraws SET tx_exec_receipt_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'), transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE trade_no = ?",
        )
        .bind("W_TX_RES_B")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records = ApiWithdrawDao::scan_need_tx_res_ack(pool.as_ref(), 100).await.unwrap();
        let trade_nos: Vec<String> = records.into_iter().map(|r| r.trade_no).collect();

        assert!(trade_nos.contains(&"W_TX_RES_A".to_string()));
        assert!(!trade_nos.contains(&"W_TX_RES_B".to_string()));
    }

    #[tokio::test]
    async fn scan_need_tx_exec_receipt_upload_allows_transaction_time_without_last_broadcast() {
        let dir = make_temp_dir("wallet_db_api_withdraw_scan_need_receipt_tx_time");
        let ctx = SqliteContext::new(&dir, Some("api_funds.db")).await.unwrap();
        let pool = ctx.into_collect_db_pool().unwrap();

        ApiWithdrawRepo::upsert_api_withdraw(
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
            "W_RECEIPT_TX_TIME",
            ApiTradeType::Withdraw,
            0,
            None,
            ApiWithdrawStatus::Init,
            ApiWithdrawStatus::Init,
            "0",
            "0",
            None,
            None,
        )
        .await
        .unwrap();

        // Chain fact exists but broadcast fact is missing (uncertain broadcast).
        sqlx::query(
            "UPDATE api_withdraws
             SET transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 tx_hash = '0xtesthash'
             WHERE trade_no = ?",
        )
        .bind("W_RECEIPT_TX_TIME")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records =
            ApiWithdrawDao::scan_need_tx_exec_receipt_upload(pool.as_ref(), 100).await.unwrap();
        let trade_nos: Vec<String> = records.into_iter().map(|r| r.trade_no).collect();

        assert!(trade_nos.contains(&"W_RECEIPT_TX_TIME".to_string()));
    }

    #[tokio::test]
    async fn scan_need_tx_exec_receipt_upload_freezes_success_missing_hash_on_chain_success() {
        let dir = make_temp_dir("wallet_db_api_withdraw_scan_receipt_freeze_chain_success");
        let ctx = SqliteContext::new(&dir, Some("api_funds.db")).await.unwrap();
        let pool = ctx.into_collect_db_pool().unwrap();

        ApiWithdrawRepo::upsert_api_withdraw(
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
            "W_RECEIPT_FREEZE_CHAIN_SUCCESS",
            ApiTradeType::Withdraw,
            0,
            None,
            ApiWithdrawStatus::Init,
            ApiWithdrawStatus::Init,
            "0",
            "0",
            None,
            None,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_withdraws
             SET chain_success_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 tx_hash = ''
             WHERE trade_no = ?",
        )
        .bind("W_RECEIPT_FREEZE_CHAIN_SUCCESS")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records =
            ApiWithdrawDao::scan_need_tx_exec_receipt_upload(pool.as_ref(), 100).await.unwrap();
        assert!(!records.iter().any(|r| r.trade_no == "W_RECEIPT_FREEZE_CHAIN_SUCCESS"));
    }

    #[tokio::test]
    async fn scan_need_tx_exec_receipt_upload_freezes_success_missing_hash_on_last_broadcast_only()
    {
        let dir = make_temp_dir("wallet_db_api_withdraw_scan_receipt_freeze_last_broadcast");
        let ctx = SqliteContext::new(&dir, Some("api_funds.db")).await.unwrap();
        let pool = ctx.into_collect_db_pool().unwrap();

        ApiWithdrawRepo::upsert_api_withdraw(
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
            "W_RECEIPT_FREEZE_LAST_BROADCAST",
            ApiTradeType::Withdraw,
            0,
            None,
            ApiWithdrawStatus::Init,
            ApiWithdrawStatus::Init,
            "0",
            "0",
            None,
            None,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_withdraws
             SET last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 tx_hash = ''
             WHERE trade_no = ?",
        )
        .bind("W_RECEIPT_FREEZE_LAST_BROADCAST")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records =
            ApiWithdrawDao::scan_need_tx_exec_receipt_upload(pool.as_ref(), 100).await.unwrap();
        assert!(!records.iter().any(|r| r.trade_no == "W_RECEIPT_FREEZE_LAST_BROADCAST"));
    }

    #[tokio::test]
    async fn scan_need_tx_exec_receipt_upload_fail_path_not_frozen_by_chain_failed() {
        let dir = make_temp_dir("wallet_db_api_withdraw_scan_receipt_fail_chain_failed");
        let ctx = SqliteContext::new(&dir, Some("api_funds.db")).await.unwrap();
        let pool = ctx.into_collect_db_pool().unwrap();

        ApiWithdrawRepo::upsert_api_withdraw(
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
            "W_RECEIPT_FAIL_CHAIN_FAILED_ALLOWED",
            ApiTradeType::Withdraw,
            0,
            None,
            ApiWithdrawStatus::Init,
            ApiWithdrawStatus::Init,
            "0",
            "0",
            None,
            None,
        )
        .await
        .unwrap();

        sqlx::query(
            "UPDATE api_withdraws
             SET chain_failed_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 tx_hash = ''
             WHERE trade_no = ?",
        )
        .bind("W_RECEIPT_FAIL_CHAIN_FAILED_ALLOWED")
        .execute(pool.as_ref())
        .await
        .unwrap();

        let records =
            ApiWithdrawDao::scan_need_tx_exec_receipt_upload(pool.as_ref(), 100).await.unwrap();
        assert!(records.iter().any(|r| r.trade_no == "W_RECEIPT_FAIL_CHAIN_FAILED_ALLOWED"));
    }
}
