use crate::entities::api_resource_delegation::{
    ApiResourceDelegationEntity, ApiResourceDelegationResultStatus, ApiResourceDelegationStatus,
    NewApiResourceDelegation,
};
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiResourceDelegationDao;

impl ApiResourceDelegationDao {
    pub async fn upsert<'a, E>(exec: E, input: NewApiResourceDelegation) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO api_resource_delegation
                (uid, source, operation_type, origin_trade_no, origin_trade_type,
                 resource_trade_no, chain_code, owner_address, receiver_address,
                 resource_type, amount, created_at, updated_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT(resource_trade_no) DO UPDATE SET
                uid = excluded.uid,
                source = excluded.source,
                operation_type = excluded.operation_type,
                origin_trade_no = excluded.origin_trade_no,
                origin_trade_type = excluded.origin_trade_type,
                chain_code = excluded.chain_code,
                owner_address = excluded.owner_address,
                receiver_address = excluded.receiver_address,
                resource_type = excluded.resource_type,
                amount = excluded.amount,
                updated_at = excluded.updated_at
        "#;

        sqlx::query(sql)
            .bind(input.uid)
            .bind(input.source.as_i64())
            .bind(input.operation_type.as_i64())
            .bind(input.origin_trade_no)
            .bind(input.origin_trade_type)
            .bind(input.resource_trade_no)
            .bind(input.chain_code)
            .bind(input.owner_address)
            .bind(input.receiver_address)
            .bind(input.resource_type.as_i64())
            .bind(input.amount)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(())
    }

    pub async fn get_by_resource_trade_no<'a, E>(
        exec: E,
        resource_trade_no: &str,
    ) -> Result<ApiResourceDelegationEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceDelegationEntity>(
            "SELECT * FROM api_resource_delegation WHERE resource_trade_no = ?",
        )
        .bind(resource_trade_no)
        .fetch_one(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list_by_origin_trade_no<'a, E>(
        exec: E,
        origin_trade_no: &str,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceDelegationEntity>(
            "SELECT * FROM api_resource_delegation WHERE origin_trade_no = ? ORDER BY id ASC",
        )
        .bind(origin_trade_no)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn mark_task_ack_sent<'a, E>(
        exec: E,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET task_ack_sent_at = COALESCE(task_ack_sent_at, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?
              AND task_ack_sent_at IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    pub async fn scan_need_task_ack<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceDelegationEntity>(
            r#"
            SELECT * FROM api_resource_delegation
            WHERE source = 1
              AND operation_type = 1
              AND task_ack_sent_at IS NULL
            ORDER BY created_at ASC
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn scan_need_result_ack<'a, E>(
        exec: E,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceDelegationEntity>(
            r#"
            SELECT * FROM api_resource_delegation
            WHERE source = 1
              AND operation_type = 1
              AND origin_trade_type = 2
              AND result_received_at IS NOT NULL
              AND result_ack_sent_at IS NULL
            ORDER BY result_received_at ASC
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn mark_result_ack_sent<'a, E>(
        exec: E,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET result_ack_sent_at = COALESCE(result_ack_sent_at, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?
              AND result_received_at IS NOT NULL
              AND result_ack_sent_at IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    pub async fn mark_result_received<'a, E>(
        exec: E,
        resource_trade_no: &str,
        result_status: ApiResourceDelegationResultStatus,
        fail_type: Option<i64>,
        err_code: Option<&str>,
        err_msg: Option<&str>,
        result_payload: Option<&str>,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET result_status = ?2,
                result_received_at = COALESCE(result_received_at, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                result_payload = COALESCE(?6, result_payload),
                fail_type = ?3,
                err_code = ?4,
                err_msg = ?5,
                status = ?7,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?1
            "#,
        )
        .bind(resource_trade_no)
        .bind(result_status.as_i64())
        .bind(fail_type)
        .bind(err_code)
        .bind(err_msg)
        .bind(result_payload)
        .bind(match result_status {
            ApiResourceDelegationResultStatus::Success => {
                ApiResourceDelegationStatus::Success.as_i64()
            }
            ApiResourceDelegationResultStatus::Fail => ApiResourceDelegationStatus::Fail.as_i64(),
        })
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }
}
