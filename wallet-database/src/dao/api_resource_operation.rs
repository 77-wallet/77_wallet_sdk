use crate::entities::api_resource_operation::{
    ApiResourceOperationEntity, NewApiResourceOperation,
};
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiResourceOperationDao;

impl ApiResourceOperationDao {
    pub async fn upsert<'a, E>(exec: E, input: NewApiResourceOperation) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO api_resource_operation
                (uid, task_source, operation_type, resource_trade_no, chain_code,
                 owner_address, receiver_address, resource_type, amount, created_at, updated_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT(resource_trade_no) DO UPDATE SET
                uid = excluded.uid,
                task_source = excluded.task_source,
                operation_type = excluded.operation_type,
                chain_code = excluded.chain_code,
                owner_address = excluded.owner_address,
                receiver_address = excluded.receiver_address,
                resource_type = excluded.resource_type,
                amount = excluded.amount,
                updated_at = excluded.updated_at
        "#;

        sqlx::query(sql)
            .bind(input.uid)
            .bind(input.task_source.as_i64())
            .bind(input.operation_type.as_i64())
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
    ) -> Result<ApiResourceOperationEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceOperationEntity>(
            "SELECT * FROM api_resource_operation WHERE resource_trade_no = ?",
        )
        .bind(resource_trade_no)
        .fetch_one(exec)
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
            UPDATE api_resource_operation
            SET task_ack_sent_at = COALESCE(task_ack_sent_at, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?
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
    ) -> Result<Vec<ApiResourceOperationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceOperationEntity>(
            r#"
            SELECT * FROM api_resource_operation
            WHERE task_source = 1
              AND task_ack_sent_at IS NULL
            ORDER BY id ASC
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }
}
