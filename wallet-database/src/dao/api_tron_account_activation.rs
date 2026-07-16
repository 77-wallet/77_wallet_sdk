use crate::entities::api_tron_account_activation::{
    ApiTronAccountActivationEntity, ApiTronAccountActivationStatus, NewApiTronAccountActivation,
};
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiTronAccountActivationDao;

impl ApiTronAccountActivationDao {
    pub async fn upsert<'a, E>(
        exec: E,
        input: NewApiTronAccountActivation,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO api_tron_account_activation
                (uid, origin_trade_no, origin_trade_type, activation_trade_no,
                 chain_code, payer_address, target_address, amount,
                 created_at, updated_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT(origin_trade_no, origin_trade_type, target_address) DO UPDATE SET
                uid = excluded.uid,
                chain_code = excluded.chain_code,
                payer_address = excluded.payer_address,
                amount = excluded.amount,
                updated_at = excluded.updated_at
        "#;

        sqlx::query(sql)
            .bind(input.uid)
            .bind(input.origin_trade_no)
            .bind(input.origin_trade_type)
            .bind(input.activation_trade_no)
            .bind(input.chain_code)
            .bind(input.payer_address)
            .bind(input.target_address)
            .bind(input.amount)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(())
    }

    pub async fn find_by_activation_trade_no<'a, E>(
        exec: E,
        activation_trade_no: &str,
    ) -> Result<Option<ApiTronAccountActivationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiTronAccountActivationEntity>(
            "SELECT * FROM api_tron_account_activation WHERE activation_trade_no = ?",
        )
        .bind(activation_trade_no)
        .fetch_optional(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn find_by_origin_target<'a, E>(
        exec: E,
        origin_trade_no: &str,
        origin_trade_type: i64,
        target_address: &str,
    ) -> Result<Option<ApiTronAccountActivationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiTronAccountActivationEntity>(
            r#"
            SELECT * FROM api_tron_account_activation
            WHERE origin_trade_no = ?
              AND origin_trade_type = ?
              AND target_address = ?
            "#,
        )
        .bind(origin_trade_no)
        .bind(origin_trade_type)
        .bind(target_address)
        .fetch_optional(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn mark_account_verified<'a, E>(
        exec: E,
        activation_trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_tron_account_activation
            SET account_verified_at = COALESCE(account_verified_at, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                status = ?2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE activation_trade_no = ?1
              AND account_verified_at IS NULL
            "#,
        )
        .bind(activation_trade_no)
        .bind(ApiTronAccountActivationStatus::Success.as_i64())
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }
}
