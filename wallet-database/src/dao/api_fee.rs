use crate::entities::api_fee::{ApiFeeEntity, ApiFeeStatus};
use chrono::SecondsFormat;
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiFeeDao;

impl ApiFeeDao {
    pub async fn all_api_fee<'a, E>(exec: E) -> Result<Vec<ApiFeeEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"SELECT * FROM api_fee"#;
        let result = sqlx::query_as::<_, ApiFeeEntity>(sql)
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
        page: i64,
        page_size: i64,
        status: ApiFeeStatus,
    ) -> Result<(i64, Vec<ApiFeeEntity>), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Clone,
    {
        let count_sql = "SELECT count(*) FROM api_fee where status=$1";
        let count = sqlx::query_scalar::<_, i64>(count_sql)
            .bind(status)
            .fetch_one(exec.clone())
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        let sql = "SELECT * FROM api_fee where status=$1 ORDER BY id ASC LIMIT $2 OFFSET $3";
        let res = sqlx::query_as::<_, ApiFeeEntity>(sql)
            .bind(status)
            .bind(page_size)
            .bind(page)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok((count, res))
    }

    async fn upsert<'c, E>(executor: E, input: ApiFeeEntity) -> Result<(), crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"
            Insert into api_fee
                (id,uid,name,from_addr,to_addr,value,chain_code,token_addr,symbol,trade_no,trade_type,status,created_at,updated_at)
            values
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            on conflict (trade_no)
            do update set
                status = excluded.status,
                updated_at = excluded.updated_at
            returning *
        "#;

        let mut rec = sqlx::query_as::<_, ApiFeeEntity>(sql)
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

    pub async fn add<'a, E>(exec: E, api_withdraw: ApiFeeEntity) -> Result<(), crate::Error>
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
        status: ApiFeeStatus,
    ) -> Result<(), crate::Error>
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

        sqlx::query(sql)
            .bind(trade_no)
            .bind(&status)
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
}
