use crate::{
    entities::{
        api_trade_type::ApiWithdrawTradeType,
        api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
    },
    pagination::Pagination,
};
use sqlx::{Executor, QueryBuilder, Sqlite};

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
            .bind(ApiWithdrawTradeType::Withdraw)
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
        qb.push_bind(ApiWithdrawTradeType::Withdraw as u8);
        if !vec_status.is_empty() {
            qb.push(" AND status IN (");
            let mut separated = qb.separated(", "); // 自动在元素间加逗号
            for status in &vec_status {
                separated.push_bind(status);
            }
            qb.push(")");
        }

        qb.push(" ORDER BY updated_at DESC, created_at DESC");
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
        count_qb.push_bind(ApiWithdrawTradeType::Withdraw as u8);
        let mut qb = QueryBuilder::<Sqlite>::new("SELECT * FROM api_withdraws WHERE trade_type = ");
        qb.push_bind(ApiWithdrawTradeType::Withdraw as u8);
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
        count_qb.push_bind(ApiWithdrawTradeType::Withdraw);
        count_qb.push(" AND init_status = ").push_bind(init_status);
        qb.push_bind(ApiWithdrawTradeType::Withdraw);
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
    ) -> Result<ApiWithdrawEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_withdraws WHERE trade_no = ? AND trade_type = ?";
        let res = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(trade_no)
            .bind(ApiWithdrawTradeType::Withdraw)
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
            .bind(ApiWithdrawTradeType::Withdraw);
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
        qb.push(" AND trade_type = ").push_bind(ApiWithdrawTradeType::SelfWithdraw);
        qb.push(" AND hash IN (");

        // 绑定多个 hash
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
        let mut qb =
            QueryBuilder::<Sqlite>::new("SELECT * FROM api_withdraws WHERE `from_addr` = ");
        qb.push_bind(ApiWithdrawTradeType::SelfWithdraw);
        if !from_addr.is_empty() {
            qb.push("from_addr = ").push_bind(from_addr);
        }
        if !chain_code.is_empty() {
            qb.push("chain_code = ").push_bind(chain_code);
        }
        if !token.is_empty() {
            qb.push("token = ").push_bind(token);
        }
        qb.push(" ORDER BY updated_at DESC, created_at DESC");
        let paginate = Pagination::<ApiWithdrawEntity>::init(page, page_size);
        Ok(paginate.page(exec, qb.sql()).await?)
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
        let mut count_qb = QueryBuilder::<Sqlite>::new(
            "SELECT count(*) FROM api_withdraws WHERE ((trade_type = 1 AND init_status = 0) OR trade_type IN (4,5)) ",
        );
        let mut qb = QueryBuilder::<Sqlite>::new(
            "SELECT * FROM api_withdraws WHERE ((trade_type = 1 AND init_status = 0) OR trade_type IN (4,5)) ",
        );
        if !uid.is_empty() {
            count_qb.push(" AND uid = ").push_bind(uid);
            qb.push(" AND uid = ").push_bind(uid);
        }
        if let Some(c) = symbol {
            count_qb.push(" AND symbol = ").push_bind(c);
            qb.push(" AND symbol = ").push_bind(c);
        }

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

    async fn upsert<'c, E>(executor: E, input: ApiWithdrawEntity) -> Result<(), crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"
            Insert into api_withdraws
                (id,uid,name,from_addr,to_addr,value,chain_code,token_addr,symbol,trade_no,trade_type,status,created_at,updated_at)
            values
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            on conflict (trade_no)
            do update set
                status = excluded.status,
                updated_at = excluded.updated_at
            returning *
        "#;

        let mut rec = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(&input.uid)
            .bind(&input.name)
            .bind(&input.from_addr)
            .bind(&input.to_addr)
            .bind(&input.value)
            .bind(&input.chain_code)
            .bind(&input.token_addr)
            .bind(&input.symbol)
            .bind(&input.trade_no)
            .bind(&input.trade_type)
            .bind(&input.status)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn add<'a, E>(exec: E, api_withdraw: ApiWithdrawEntity) -> Result<(), crate::Error>
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
                ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
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

    pub async fn update_status<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiWithdrawStatus,
        notes: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                status = $2,
                notes = $3,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;

        sqlx::query(sql)
            .bind(trade_no)
            .bind(&status)
            .bind(notes)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn update_next_status<'a, E>(
        exec: E,
        trade_no: &str,
        status: ApiWithdrawStatus,
        next_status: ApiWithdrawStatus,
        notes: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                status = $3,
                 notes = $4,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1 and status = $2
        "#;

        let res = sqlx::query(sql)
            .bind(trade_no)
            .bind(&status)
            .bind(&next_status)
            .bind(notes)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    pub async fn update_tx_status<'a, E>(
        exec: E,
        trade_no: &str,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        block_height: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                status = $2,
                tx_hash = $3,
                resource_consume = $4,
                transaction_fee = $5,
                transaction_time = $6,
                block_height = $7,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
        "#;

        sqlx::query(sql)
            .bind(trade_no)
            .bind(status)
            .bind(tx_hash)
            .bind(resource_consume)
            .bind(transaction_fee)
            .bind(transaction_time)
            .bind(block_height)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
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
}
