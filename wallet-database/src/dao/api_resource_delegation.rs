use crate::entities::api_resource_delegation::{
    ApiResourceDelegationEntity, ApiResourceDelegationOperationType,
    ApiResourceDelegationRecoverStatus, ApiResourceDelegationResultStatus,
    ApiResourceDelegationSource, ApiResourceDelegationStatus, NewApiResourceDelegation,
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
                 delegation_mode, permission_id, resource_type, native_amount, amount,
                 created_at, updated_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
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
                delegation_mode = excluded.delegation_mode,
                permission_id = excluded.permission_id,
                resource_type = excluded.resource_type,
                native_amount = excluded.native_amount,
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
            .bind(input.delegation_mode.as_i64())
            .bind(input.permission_id)
            .bind(input.resource_type.as_i64())
            .bind(input.native_amount)
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

    pub async fn scan_need_task_ack_by_origin_type<'a, E>(
        exec: E,
        origin_trade_type: i64,
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
              AND origin_trade_type = ?
              AND task_ack_sent_at IS NULL
              AND (next_retry_at IS NULL OR next_retry_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ORDER BY created_at ASC
            LIMIT ?
            "#,
        )
        .bind(origin_trade_type)
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn scan_need_task_ack_by_origin_type_source_and_operation<'a, E>(
        exec: E,
        origin_trade_type: i64,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceDelegationEntity>(
            r#"
            SELECT * FROM api_resource_delegation
            WHERE source = ?
              AND operation_type = ?
              AND origin_trade_type = ?
              AND task_ack_sent_at IS NULL
              AND (next_retry_at IS NULL OR next_retry_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ORDER BY created_at ASC, id ASC
            LIMIT ?
            "#,
        )
        .bind(source.as_i64())
        .bind(operation_type.as_i64())
        .bind(origin_trade_type)
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn scan_need_result_ack_by_origin_type<'a, E>(
        exec: E,
        origin_trade_type: i64,
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
              AND origin_trade_type = ?
              AND result_received_at IS NOT NULL
              AND result_payload IS NOT NULL
              AND result_ack_sent_at IS NULL
              AND (next_retry_at IS NULL OR next_retry_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ORDER BY result_received_at ASC
            LIMIT ?
            "#,
        )
        .bind(origin_trade_type)
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn find_pending_result_ack_by_origin<'a, E>(
        exec: E,
        origin_trade_type: i64,
        origin_trade_no: &str,
    ) -> Result<Option<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceDelegationEntity>(
            r#"
            SELECT * FROM api_resource_delegation
            WHERE source = 1
              AND operation_type = 1
              AND origin_trade_type = ?
              AND origin_trade_no = ?
              AND result_received_at IS NOT NULL
              AND result_payload IS NOT NULL
              AND result_ack_sent_at IS NULL
              AND (next_retry_at IS NULL OR next_retry_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ORDER BY result_received_at ASC, id ASC
            LIMIT 1
            "#,
        )
        .bind(origin_trade_type)
        .bind(origin_trade_no)
        .fetch_optional(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn scan_need_result_ack_by_source_and_operation<'a, E>(
        exec: E,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceDelegationEntity>(
            r#"
            SELECT * FROM api_resource_delegation
            WHERE source = ?
              AND operation_type = ?
              AND result_received_at IS NOT NULL
              AND result_payload IS NOT NULL
              AND result_ack_sent_at IS NULL
              AND (next_retry_at IS NULL OR next_retry_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ORDER BY result_received_at ASC
            LIMIT ?
            "#,
        )
        .bind(source.as_i64())
        .bind(operation_type.as_i64())
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn scan_can_execute_by_origin_type<'a, E>(
        exec: E,
        origin_trade_type: i64,
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
              AND origin_trade_type = ?
              AND status = 1
              AND task_ack_sent_at IS NOT NULL
              AND building_at IS NULL
              AND tx_hash IS NULL
              AND tx_status IS NULL
            ORDER BY task_ack_sent_at ASC
            LIMIT ?
            "#,
        )
        .bind(origin_trade_type)
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn scan_can_execute_by_origin_type_and_source<'a, E>(
        exec: E,
        origin_trade_type: i64,
        source: ApiResourceDelegationSource,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let (ack_clause, order_expr) = match source {
            ApiResourceDelegationSource::Platform => {
                ("AND task_ack_sent_at IS NOT NULL", "task_ack_sent_at")
            }
            ApiResourceDelegationSource::Local => ("", "created_at"),
        };
        let sql = format!(
            r#"
            SELECT * FROM api_resource_delegation
            WHERE source = ?
              AND operation_type = 1
              AND origin_trade_type = ?
              AND status = 1
              {ack_clause}
              AND building_at IS NULL
              AND tx_hash IS NULL
              AND tx_status IS NULL
            ORDER BY {order_expr} ASC
            LIMIT ?
            "#
        );
        sqlx::query_as::<_, ApiResourceDelegationEntity>(&sql)
            .bind(source.as_i64())
            .bind(origin_trade_type)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn find_by_resource_trade_no<'a, E>(
        exec: E,
        resource_trade_no: &str,
    ) -> Result<Option<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceDelegationEntity>(
            "SELECT * FROM api_resource_delegation WHERE resource_trade_no = ?",
        )
        .bind(resource_trade_no)
        .fetch_optional(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn scan_can_execute_by_origin_type_source_and_operation<'a, E>(
        exec: E,
        origin_trade_type: i64,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let (ack_clause, order_expr) = match source {
            ApiResourceDelegationSource::Platform => {
                ("AND task_ack_sent_at IS NOT NULL", "task_ack_sent_at")
            }
            ApiResourceDelegationSource::Local => ("", "created_at"),
        };
        let sql = format!(
            r#"
            SELECT * FROM api_resource_delegation
            WHERE source = ?
              AND operation_type = ?
              AND origin_trade_type = ?
              AND status = 1
              AND result_received_at IS NULL
              AND err_code IS NULL
              {ack_clause}
              AND (next_retry_at IS NULL OR next_retry_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
              AND (
                    building_at IS NULL
                    OR (
                      operation_type = 2
                      AND datetime(building_at) <= datetime('now', '-5 minutes')
                    )
                  )
              AND tx_hash IS NULL
            ORDER BY {order_expr} ASC, id ASC
            LIMIT ?
            "#
        );
        sqlx::query_as::<_, ApiResourceDelegationEntity>(&sql)
            .bind(source.as_i64())
            .bind(operation_type.as_i64())
            .bind(origin_trade_type)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn scan_can_recover_local_undelegation_by_origin_type<'a, E>(
        exec: E,
        origin_trade_type: i64,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceDelegationEntity>(
            r#"
            SELECT * FROM api_resource_delegation
            WHERE source = 2
              AND operation_type = 2
              AND origin_trade_type = ?
              AND result_received_at IS NULL
              AND err_code IS NULL
              AND tx_hash IS NOT NULL
              AND trim(tx_hash) <> ''
              AND (next_retry_at IS NULL OR next_retry_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ORDER BY COALESCE(next_retry_at, created_at) ASC, id ASC
            LIMIT ?
            "#,
        )
        .bind(origin_trade_type)
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn scan_can_recover_by_origin_type_source_and_operation<'a, E>(
        exec: E,
        origin_trade_type: i64,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let (ack_clause, order_expr) = match source {
            ApiResourceDelegationSource::Platform => {
                ("AND task_ack_sent_at IS NOT NULL", "task_ack_sent_at")
            }
            ApiResourceDelegationSource::Local => ("", "created_at"),
        };
        let sql = format!(
            r#"
            SELECT * FROM api_resource_delegation
            WHERE source = ?
              AND operation_type = ?
              AND origin_trade_type = ?
              AND result_received_at IS NULL
              AND err_code IS NULL
              AND tx_hash IS NOT NULL
              AND trim(tx_hash) <> ''
              {ack_clause}
              AND (next_retry_at IS NULL OR next_retry_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ORDER BY COALESCE(next_retry_at, {order_expr}) ASC, id ASC
            LIMIT ?
            "#
        );
        sqlx::query_as::<_, ApiResourceDelegationEntity>(&sql)
            .bind(source.as_i64())
            .bind(operation_type.as_i64())
            .bind(origin_trade_type)
            .bind(limit as i64)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn claim_build_slot<'a, E>(
        exec: E,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET building_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                next_retry_at = NULL,
                recover_status = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?
              AND status = 1
              AND (
                    source = 2
                    OR task_ack_sent_at IS NOT NULL
                  )
              AND result_received_at IS NULL
              AND err_code IS NULL
              AND (next_retry_at IS NULL OR next_retry_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
              AND (
                    building_at IS NULL
                    OR (
                      operation_type = 2
                      AND datetime(building_at) <= datetime('now', '-5 minutes')
                    )
                  )
              AND tx_hash IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    pub async fn mark_broadcast_success<'a, E>(
        exec: E,
        resource_trade_no: &str,
        tx_hash: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET tx_hash = ?2,
                tx_status = 'success',
                next_retry_at = NULL,
                recover_status = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?1
              AND status = 1
              AND building_at IS NOT NULL
              AND tx_hash IS NULL
              AND result_received_at IS NULL
              AND err_code IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .bind(tx_hash)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    pub async fn scan_need_tx_exec_receipt_upload_by_origin_type<'a, E>(
        exec: E,
        origin_trade_type: i64,
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
              AND origin_trade_type = ?
              AND tx_exec_receipt_uploaded_at IS NULL
              AND (
                    (
                      tx_status = 'success'
                      AND tx_hash IS NOT NULL
                      AND trim(tx_hash) <> ''
                    )
                    OR err_code IS NOT NULL
                  )
            ORDER BY updated_at ASC, id ASC
            LIMIT ?
            "#,
        )
        .bind(origin_trade_type)
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn scan_need_tx_exec_receipt_upload_by_origin_type_source_and_operation<'a, E>(
        exec: E,
        origin_trade_type: i64,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceDelegationEntity>(
            r#"
            SELECT * FROM api_resource_delegation
            WHERE source = ?
              AND operation_type = ?
              AND origin_trade_type = ?
              AND tx_exec_receipt_uploaded_at IS NULL
              AND (
                    (
                      tx_status = 'success'
                      AND tx_hash IS NOT NULL
                      AND trim(tx_hash) <> ''
                    )
                    OR err_code IS NOT NULL
                  )
            ORDER BY updated_at ASC, id ASC
            LIMIT ?
            "#,
        )
        .bind(source.as_i64())
        .bind(operation_type.as_i64())
        .bind(origin_trade_type)
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn scan_need_tx_exec_receipt_upload_by_source_and_operation<'a, E>(
        exec: E,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as::<_, ApiResourceDelegationEntity>(
            r#"
            SELECT * FROM api_resource_delegation
            WHERE source = ?
              AND operation_type = ?
              AND tx_exec_receipt_uploaded_at IS NULL
              AND (
                    (
                      tx_status = 'success'
                      AND tx_hash IS NOT NULL
                      AND trim(tx_hash) <> ''
                    )
                    OR err_code IS NOT NULL
                  )
            ORDER BY updated_at ASC, id ASC
            LIMIT ?
            "#,
        )
        .bind(source.as_i64())
        .bind(operation_type.as_i64())
        .bind(limit as i64)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn mark_tx_exec_receipt_uploaded<'a, E>(
        exec: E,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET tx_exec_receipt_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?
              AND source = 1
              AND operation_type = 1
              AND tx_exec_receipt_uploaded_at IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    pub async fn mark_tx_exec_receipt_uploaded_by_source_and_operation<'a, E>(
        exec: E,
        resource_trade_no: &str,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET tx_exec_receipt_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?
              AND source = ?
              AND operation_type = ?
              AND tx_exec_receipt_uploaded_at IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .bind(source.as_i64())
        .bind(operation_type.as_i64())
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    pub async fn mark_failed_if_unfinished<'a, E>(
        exec: E,
        resource_trade_no: &str,
        err_code: &str,
        err_msg: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET err_code = ?2,
                err_msg = ?3,
                tx_status = 'fail',
                status = 3,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?1
              AND result_received_at IS NULL
              AND err_code IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .bind(err_code)
        .bind(err_msg)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    pub async fn mark_task_ack_retry_wait<'a, E>(
        exec: E,
        resource_trade_no: &str,
        next_retry_at: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET next_retry_at = ?2,
                retry_count = retry_count + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?1
              AND task_ack_sent_at IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .bind(next_retry_at)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    pub async fn mark_result_ack_retry_wait<'a, E>(
        exec: E,
        resource_trade_no: &str,
        next_retry_at: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET next_retry_at = ?2,
                retry_count = retry_count + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?1
              AND result_received_at IS NOT NULL
              AND result_payload IS NOT NULL
              AND result_ack_sent_at IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .bind(next_retry_at)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
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
              AND result_payload IS NOT NULL
              AND result_ack_sent_at IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    pub async fn upsert_original_order_result_fact<'a, E>(
        exec: E,
        input: NewApiResourceDelegation,
        result_status: ApiResourceDelegationResultStatus,
        fail_type: Option<i64>,
        result_payload: Option<&str>,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let status = match result_status {
            ApiResourceDelegationResultStatus::Success => ApiResourceDelegationStatus::Success,
            ApiResourceDelegationResultStatus::Fail => ApiResourceDelegationStatus::Fail,
        };
        let res = sqlx::query(
            r#"
            INSERT INTO api_resource_delegation
                (uid, source, operation_type, origin_trade_no, origin_trade_type,
                 resource_trade_no, chain_code, owner_address, receiver_address,
                 delegation_mode, permission_id, resource_type, native_amount, amount,
                 status, task_ack_sent_at,
                 result_status, result_received_at, result_payload, fail_type,
                 created_at, updated_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                 ?15,
                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                 ?16,
                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                 ?17, ?18,
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
                delegation_mode = excluded.delegation_mode,
                permission_id = excluded.permission_id,
                resource_type = excluded.resource_type,
                native_amount = excluded.native_amount,
                amount = excluded.amount,
                status = excluded.status,
                task_ack_sent_at = COALESCE(
                    api_resource_delegation.task_ack_sent_at,
                    excluded.task_ack_sent_at
                ),
                result_status = excluded.result_status,
                result_received_at = COALESCE(
                    api_resource_delegation.result_received_at,
                    excluded.result_received_at
                ),
                result_payload = COALESCE(excluded.result_payload, api_resource_delegation.result_payload),
                fail_type = excluded.fail_type,
                next_retry_at = NULL,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(input.uid)
        .bind(input.source.as_i64())
        .bind(input.operation_type.as_i64())
        .bind(input.origin_trade_no)
        .bind(input.origin_trade_type)
        .bind(input.resource_trade_no)
        .bind(input.chain_code)
        .bind(input.owner_address)
        .bind(input.receiver_address)
        .bind(input.delegation_mode.as_i64())
        .bind(input.permission_id)
        .bind(input.resource_type.as_i64())
        .bind(input.native_amount)
        .bind(input.amount)
        .bind(status.as_i64())
        .bind(result_status.as_i64())
        .bind(result_payload)
        .bind(fail_type)
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
                recover_status = NULL,
                next_retry_at = NULL,
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

    pub async fn mark_recover_retry_wait<'a, E>(
        exec: E,
        resource_trade_no: &str,
        recover_status: ApiResourceDelegationRecoverStatus,
        next_retry_at: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET recover_status = ?2,
                next_retry_at = ?3,
                retry_count = retry_count + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?1
              AND operation_type = 2
              AND result_received_at IS NULL
              AND err_code IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .bind(recover_status.as_i64())
        .bind(next_retry_at)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    pub async fn reset_for_retry<'a, E>(
        exec: E,
        resource_trade_no: &str,
        recover_status: ApiResourceDelegationRecoverStatus,
        next_retry_at: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET status = 1,
                building_at = NULL,
                tx_hash = NULL,
                tx_status = NULL,
                recover_status = ?2,
                next_retry_at = ?3,
                retry_count = retry_count + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = ?1
              AND operation_type IN (1, 2)
              AND result_received_at IS NULL
              AND err_code IS NULL
            "#,
        )
        .bind(resource_trade_no)
        .bind(recover_status.as_i64())
        .bind(next_retry_at)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }
}
