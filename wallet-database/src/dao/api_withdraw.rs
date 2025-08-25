use crate::dao::Dao;
use crate::entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus};
use chrono::SecondsFormat;
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiWithdrawDao;

#[async_trait::async_trait]
impl Dao for ApiWithdrawDao {
    type Input = ApiWithdrawEntity;
    type Output = ();
    type Error = crate::Error;

    async fn upsert<'c, E>(executor: E, input: Self::Input) -> Result<Self::Output, Self::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"
            Insert into api_withdraws
                (id,uid,name,from_addr,to_addr,value,decimals,token_addr,symbol,trade_no,trade_type,status,created_at,updated_at)
            values
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            on conflict (trade_no)
            do update set
                status = excluded.status,
                updated_at = excluded.updated_at
            returning *
        "#;

        let mut rec = sqlx::query_as::<_, Self::Input>(sql)
            .bind(&input.uid)
            .bind(&input.name)
            .bind(&input.from_addr)
            .bind(&input.to_addr)
            .bind(&input.value)
            .bind(&input.decimals)
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

    async fn list<'c, E>(executor: E) -> Result<Vec<Self::Output>, Self::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        todo!()
    }
}

impl ApiWithdrawDao {
    pub async fn add<'a, E>(exec: E, api_withdraw: ApiWithdrawEntity) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO api_withdraws
                (uid,name,from_addr,to_addr,value,decimals,token_addr,symbol,trade_no,trade_type,status,tx_hash,send_tx_at,created_at,updated_at)
            VALUES
                (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
        "#;

        sqlx::query(sql)
            .bind(&api_withdraw.uid)
            .bind(&api_withdraw.name)
            .bind(&api_withdraw.from_addr)
            .bind(&api_withdraw.to_addr)
            .bind(&api_withdraw.value)
            .bind(&api_withdraw.decimals)
            .bind(&api_withdraw.token_addr)
            .bind(&api_withdraw.symbol)
            .bind(&api_withdraw.trade_no)
            .bind(&api_withdraw.trade_type)
            .bind(0)
            .bind("")
            .bind(0)
            .bind(
                api_withdraw
                    .created_at
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
            )
            .bind(0)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn update<'a, E>(exec: E, api_withdraw: ApiWithdrawEntity) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                name = ?,
                threshold = ?,
                member = ?,
                chain_code = ?,
                operations = ?,
                is_del = ?,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = ?
        "#;
        tracing::warn!("{:#?}", api_withdraw);

        sqlx::query(sql)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn update_status<'a, E>(
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
                name = ?,
                threshold = ?,
                member = ?,
                chain_code = ?,
                operations = ?,
                is_del = ?,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = ?
        "#;

        sqlx::query(sql)
            .bind(trade_no)
            .bind(&status)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn all_api_withdraw<'a, E>(exec: E) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"SELECT * FROM api_withdraws"#;
        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    pub async fn page_api_withdraw<'a, E>(
        exec: E,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let count_sql = "SELECT count(*) FROM";
        let sql = "SELECT * FROM api_withdraws ORDER BY created_at DESC LIMIT ? OFFSET ?";
        // let paginate = Pagination::<Self>::init(page, page_size);
        // Ok(paginate.page(exec, sql).await?)
        Ok(vec![])
    }
}
