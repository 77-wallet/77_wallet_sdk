use sqlx::{Executor, Sqlite};

use crate::entities::{
    expand_batch::{CreateExpandBatchEntity, ExpandBatchEntity, ExpandBatchStatus},
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
            (uid, batch_id, serial_no, chain_code, total_count, finished_count, status, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, 0, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
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
            .bind(ExpandBatchStatus::Running)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 原子增加已完成计数，支持一次性增加指定数量
    pub async fn increment_finished<'a, E>(
        exec: E,
        batch_id: &str,
        increment: u64,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE expand_batch 
            SET 
                finished_count = MIN(finished_count + ?, total_count),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE batch_id = ? AND status = ?
        "#;

        sqlx::query(sql)
            .bind(increment as i32)
            .bind(batch_id)
            .bind(ExpandBatchStatus::Running)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 原子增加已完成计数，每次增加1个（兼容旧接口）
    pub async fn increment_finished_one<'a, E>(exec: E, batch_id: &str) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        Self::increment_finished(exec, batch_id, 1).await
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
        SELECT *
        FROM expand_batch
        WHERE status = ?
    "#;

        sqlx::query_as(sql)
            .bind(ExpandBatchStatus::Running)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
