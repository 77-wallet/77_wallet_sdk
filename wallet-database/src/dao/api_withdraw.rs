use crate::entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus};
use chrono::SecondsFormat;
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiWithdrawDao;

impl ApiWithdrawDao {
    pub async fn all_api_withdraw<'a, E>(
        exec: E,
        uid: &str,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"SELECT * FROM api_withdraws where uid = ?"#;
        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(uid)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    pub async fn page_api_withdraw<'a, E>(
        exec: E,
        uid: &str,
        page: i64,
        page_size: i64,
    ) -> Result<(i64, Vec<ApiWithdrawEntity>), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Clone,
    {
        let count_sql = "SELECT count(*) FROM api_withdraws where uid = ?";
        let count = sqlx::query_scalar::<_, i64>(count_sql)
            .bind(uid)
            .fetch_one(exec.clone())
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        let sql =
            "SELECT * FROM api_withdraws where uid = ? ORDER BY created_at DESC LIMIT ? OFFSET ?";
        let res = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(uid)
            .bind(page_size)
            .bind(page)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok((count, res))
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
        let count_sql =
            format!("SELECT count(*) FROM api_withdraws where status in ({})", placeholders);
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

        // tracing::info!(status=%vec_status[0], "sql: {}", sql);
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

    pub async fn get_api_withdraw_by_trade_no<'a, E>(
        exec: E,
        trade_no: &str,
    ) -> Result<ApiWithdrawEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_withdraws WHERE trade_no = ?";
        let res = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(trade_no)
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
        let sql =
            format!("SELECT * FROM api_withdraws where trade_no = ? AND status in ({})", placeholders);
        let mut query = sqlx::query_as::<_, ApiWithdrawEntity>(&sql)
            .bind(trade_no);
        for status in vec_status {
            query = query.bind(status);
        }
        let res= query
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
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
                chain_code,
                token_addr,
                symbol,
                trade_no,
                trade_type,
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
                ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        "#;

        let res = sqlx::query(sql)
            .bind(&api_withdraw.uid)
            .bind(&api_withdraw.name)
            .bind(&api_withdraw.from_addr)
            .bind(&api_withdraw.to_addr)
            .bind(&api_withdraw.value)
            .bind(&api_withdraw.chain_code)
            .bind(&api_withdraw.token_addr)
            .bind(&api_withdraw.symbol)
            .bind(&api_withdraw.trade_no)
            .bind(&api_withdraw.trade_type)
            .bind(&api_withdraw.status)
            .bind("") // hash
            .bind(0) // consume
            .bind(0) // fee
            .bind(api_withdraw.created_at.to_rfc3339_opts(SecondsFormat::Secs, true))
            .bind(0)
            .bind(&api_withdraw.notes)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        tracing::info!(xx=%res.rows_affected(), "withdraw api");
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
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                status = $3,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1 and status = $2
        "#;

        sqlx::query(sql)
            .bind(trade_no)
            .bind(&status)
            .bind(&next_status)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn update_tx_status<'a, E>(
        exec: E,
        trade_no: &str,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
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
