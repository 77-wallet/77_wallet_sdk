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

    pub async fn scan_can_build<'a, E>(
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
              AND task_ack_sent_at IS NOT NULL
              AND building_at IS NULL
              AND raw_tx IS NULL
              AND err_code IS NULL
            ORDER BY id ASC
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn scan_can_broadcast<'a, E>(
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
              AND raw_tx IS NOT NULL
              AND trim(raw_tx) <> ''
              AND tx_hash IS NOT NULL
              AND trim(tx_hash) <> ''
              AND last_broadcast_at IS NULL
              AND err_code IS NULL
            ORDER BY id ASC
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn claim_building_at<'a, E>(
        exec: E,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_operation
            SET building_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?
              AND task_source = 1
              AND task_ack_sent_at IS NOT NULL
              AND building_at IS NULL
              AND raw_tx IS NULL
              AND err_code IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// Persist the irreversible build facts for a resource operation.
    ///
    /// raw_tx is the first executable chain fact. Once it exists, scanner must
    /// stop treating the operation as buildable and later broadcast/recover
    /// logic must use this stored payload instead of rebuilding implicitly.
    pub async fn update_after_build<'a, E>(
        exec: E,
        resource_trade_no: &str,
        tx_hash: &str,
        raw_tx: &str,
        transaction_fee: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_operation
            SET raw_tx = ?3,
                tx_hash = ?2,
                transaction_fee = ?4,
                building_at = COALESCE(building_at, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?1
              AND task_ack_sent_at IS NOT NULL
              AND raw_tx IS NULL
              AND err_code IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .bind(tx_hash)
        .bind(raw_tx)
        .bind(transaction_fee)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// Release an in-flight build slot after a pre-raw_tx failure or early exit.
    pub async fn clear_building_at<'a, E>(
        exec: E,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_operation
            SET building_at = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?
              AND raw_tx IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    /// Mark a resource operation as broadcast to the chain node.
    ///
    /// This is only a broadcast fact. Chain confirmation must be written by a
    /// later recover/confirm step, never by this method.
    pub async fn mark_broadcast_executed<'a, E>(
        exec: E,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_operation
            SET last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?
              AND task_source = 1
              AND raw_tx IS NOT NULL
              AND trim(raw_tx) <> ''
              AND tx_hash IS NOT NULL
              AND trim(tx_hash) <> ''
              AND last_broadcast_at IS NULL
              AND err_code IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }
}
