use sqlx::{Executor, Sqlite};

use crate::entities::{
    expand_batch::{BatchWithCount, CreateExpandBatchEntity, ExpandBatchEntity, ExpandBatchStatus},
    expand_batch_item::ExpandItemStatus,
};

pub struct ExpandBatchDao {}

impl ExpandBatchDao {
    /// 创建新的扩容批次
    pub async fn create<'a, E>(exec: E, req: CreateExpandBatchEntity) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO expand_batch 
            (uid, batch_id, serial_no, chain_code, total_count, finished_count, retry_count, status, created_at)
            VALUES (?, ?, ?, ?, ?, 0, 0, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT (batch_id) DO UPDATE SET 
                uid = excluded.uid,
                serial_no = excluded.serial_no,
                chain_code = excluded.chain_code,
                total_count = MAX(total_count, excluded.total_count),
                updated_at = excluded.updated_at
        "#;

        sqlx::query(sql)
            .bind(&req.uid)
            .bind(&req.batch_id)
            .bind(&req.serial_no)
            .bind(&req.chain_code)
            .bind(req.total_count)
            .bind(ExpandBatchStatus::Pending)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 获取批次信息
    pub async fn get_batch<'a, E>(
        exec: E,
        batch_id: &str,
    ) -> Result<Option<ExpandBatchEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM expand_batch WHERE batch_id = ?
        "#;

        sqlx::query_as::<sqlx::Sqlite, ExpandBatchEntity>(sql)
            .bind(batch_id)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 检查批次是否已完成
    pub async fn is_batch_done<'a, E>(exec: E, batch_id: &str) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT status = ? AS is_done FROM expand_batch WHERE batch_id = ?
        "#;

        let is_done: Option<bool> = sqlx::query_scalar(sql)
            .bind(ExpandBatchStatus::Done)
            .bind(batch_id)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(is_done.unwrap_or(false))
    }

    pub async fn get_running_batches_with_insufficient_items<'a, E>(
        exec: E,
        uid: &str,
        chain: &str,
    ) -> Result<Vec<BatchWithCount>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        //   SELECT b.*, COUNT(i.batch_id) as item_count
        // FROM expand_batch b
        // LEFT JOIN expand_batch_item i
        //   ON b.batch_id = i.batch_id
        // WHERE b.uid = ?
        //   AND b.chain_code = ?
        //   AND b.status IN (?, ?)
        // GROUP BY b.batch_id
        // HAVING item_count < b.total_count
        let sql = r#"
            SELECT
            b.batch_id,
            b.uid,
            b.chain_code,
            b.serial_no,
            b.total_count,
            b.finished_count,
            b.status,
            b.retry_count,
            b.created_at,
            b.updated_at,
            COALESCE(i.item_count, 0) AS item_count
        FROM expand_batch b
        LEFT JOIN (
            SELECT
                batch_id,
                COUNT(input_index) AS item_count
            FROM expand_batch_item
            GROUP BY batch_id
        ) i ON b.batch_id = i.batch_id
        WHERE b.uid = ?
        AND b.chain_code = ?
        AND b.status IN (?, ?)
        AND COALESCE(i.item_count, 0) < b.total_count
    "#;

        sqlx::query_as::<_, BatchWithCount>(sql)
            .bind(uid)
            .bind(chain)
            .bind(ExpandBatchStatus::Running)
            .bind(ExpandBatchStatus::Failed)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn update_status<'a, E>(
        exec: E,
        batch_id: &str,
        from: ExpandBatchStatus,
        to: ExpandBatchStatus,
    ) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE expand_batch
        SET status = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE batch_id = ?
          AND status = ?
    "#;

        let res = sqlx::query(sql)
            .bind(to)
            .bind(batch_id)
            .bind(from)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected() > 0)
    }

    pub async fn mark_done_if_finished<'a, E>(exec: E, batch_id: &str) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE expand_batch
        SET status = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE batch_id = ?
          AND status = ?
          AND finished_count >= total_count
    "#;

        let res = sqlx::query(sql)
            .bind(ExpandBatchStatus::Done)
            .bind(batch_id)
            .bind(ExpandBatchStatus::Running)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected() > 0)
    }

    pub async fn recompute_finished_count<'a, E>(
        exec: E,
        uid: &str,
        chain: &str,
    ) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE expand_batch
            SET finished_count = (
                SELECT COUNT(*) FROM expand_batch_item i
                WHERE i.batch_id = expand_batch.batch_id
                  AND i.status = ?
            ),
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE uid = ?
              AND chain_code = ?
              AND status = ?
              AND finished_count < total_count
        "#;

        let res = sqlx::query(sql)
            .bind(ExpandItemStatus::Done)
            .bind(uid)
            .bind(chain)
            .bind(ExpandBatchStatus::Running)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected() > 0)
    }

    /// 检查批次是否已通知后端完成
    pub async fn is_batch_notified<'a, E>(exec: E, batch_id: &str) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT status = ? AS is_notified 
            FROM expand_batch 
            WHERE batch_id = ?
        "#;

        let is_notified: Option<bool> = sqlx::query_scalar(sql)
            .bind(ExpandBatchStatus::Notified)
            .bind(batch_id)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(is_notified.unwrap_or(false))
    }

    pub async fn get_by_status<'a, E>(
        exec: E,
        status: ExpandBatchStatus,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        SELECT * FROM expand_batch
        WHERE status = ?
    "#;

        sqlx::query_as::<sqlx::Sqlite, ExpandBatchEntity>(sql)
            .bind(status)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 获取已完成但未通知后端的批次
    pub async fn get_done_but_not_notified<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM expand_batch 
            WHERE status = ? 
                AND uid = ?
                AND chain_code = ?
                AND expand_complete_at IS NOT NULL
        "#;

        sqlx::query_as::<sqlx::Sqlite, ExpandBatchEntity>(sql)
            .bind(ExpandBatchStatus::Done)
            .bind(uid)
            .bind(chain_code)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_all_finished_but_running<'a, E>(
        exec: E,
        uid: &str,
        chain: &str,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        SELECT * FROM expand_batch
        WHERE uid = ?
          AND chain_code = ?
          AND status = ?
          AND finished_count >= total_count
    "#;

        sqlx::query_as::<_, ExpandBatchEntity>(sql)
            .bind(uid)
            .bind(chain)
            .bind(ExpandBatchStatus::Running)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 找出所有未完成的 batch（finished < total）
    pub async fn get_unfinished_batches<'a, E>(
        exec: E,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        SELECT * FROM expand_batch
        WHERE finished_count < total_count
         AND status = ?
    "#;

        sqlx::query_as::<sqlx::Sqlite, ExpandBatchEntity>(sql)
            .bind(ExpandBatchStatus::Running)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 获取所有批次
    pub async fn get_all<'a, E>(exec: E) -> Result<Vec<ExpandBatchEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM expand_batch";

        sqlx::query_as::<sqlx::Sqlite, ExpandBatchEntity>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 更新批次的finished_count缓存
    pub async fn update_finished_count<'a, E>(
        exec: E,
        batch_id: &str,
        count: i64,
    ) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE expand_batch
        SET finished_count = ?, 
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE batch_id = ?
        "#;

        let res = sqlx::query(sql)
            .bind(count)
            .bind(batch_id)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected() > 0)
    }

    /// 将批次状态从Pending转为Running，使用CAS确保只有一个实例能成功
    pub async fn mark_running_if_pending<'a, E>(
        exec: E,
        batch_id: &str,
    ) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE expand_batch
        SET status = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE batch_id = ?
          AND status = ?
        "#;

        let res = sqlx::query(sql)
            .bind(ExpandBatchStatus::Running)
            .bind(batch_id)
            .bind(ExpandBatchStatus::Pending)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected() > 0)
    }

    /// 更新expand_complete_at字段，仅当它为NULL时
    pub async fn update_expand_complete_at_if_null<'a, E>(
        exec: E,
        batch_id: &str,
    ) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE expand_batch
        SET expand_complete_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE batch_id = ?
          AND expand_complete_at IS NULL
        "#;

        let res = sqlx::query(sql)
            .bind(batch_id)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected() > 0)
    }

    /// 将批次状态从Done转为Notified，使用CAS确保只有一个实例能成功
    pub async fn done_to_notified_if_match<'a, E>(
        exec: E,
        batch_id: &str,
    ) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE expand_batch
        SET status = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE batch_id = ?
          AND status = ?
          AND expand_complete_at IS NOT NULL
        "#;

        let res = sqlx::query(sql)
            .bind(ExpandBatchStatus::Notified)
            .bind(batch_id)
            .bind(ExpandBatchStatus::Done)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected() > 0)
    }
}
