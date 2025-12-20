use sqlx::{Executor, Sqlite};

use crate::entities::expand_batch::{CreateExpandBatchEntity, ExpandBatchEntity};

pub struct ExpandBatchDao {}

impl ExpandBatchDao {
    /// 创建新的扩容批次
    pub async fn create<'a, E>(exec: E, req: CreateExpandBatchEntity) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO expand_batch 
            (uid, batch_id, serial_no, chain_code, total_count, finished_count, status, notified_complete, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, 0, 0, 0, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT (batch_id) DO UPDATE SET 
                uid = excluded.uid,
                serial_no = excluded.serial_no,
                chain_code = excluded.chain_code,
                total_count = MAX(total_count, excluded.total_count),
                updated_at = excluded.updated_at
        "#;

        sqlx::query(sql)
            .bind(&req.batch_id)
            .bind(&req.chain_code)
            .bind(req.total_count)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 原子增加已完成计数，支持一次性增加指定数量
    pub async fn increment_finished<'a, E>(
        exec: E,
        batch_id: &str,
        increment: usize,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE expand_batch 
            SET 
                finished_count = MIN(finished_count + ?, total_count),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE batch_id = ?
        "#;

        sqlx::query(sql)
            .bind(increment as i32)
            .bind(batch_id)
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

    /// 标记批次为完成
    pub async fn mark_as_done<'a, E>(exec: E, batch_id: &str) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE expand_batch 
            SET 
                status = 1,
                finished_count = total_count,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE batch_id = ?
                AND status = 0
                AND finished_count >= total_count
        "#;

        let result = sqlx::query(sql)
            .bind(batch_id)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        // 返回影响的行数，让调用者知道更新是否成功
        Ok(result.rows_affected())
    }

    /// 检查批次是否已完成
    pub async fn is_batch_done<'a, E>(exec: E, batch_id: &str) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT finished_count >= total_count AS is_done 
            FROM expand_batch 
            WHERE batch_id = ?
        "#;

        let is_done: Option<bool> = sqlx::query_scalar(sql)
            .bind(batch_id)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(is_done.unwrap_or(false))
    }

    /// 标记批次已通知后端完成
    pub async fn mark_as_notified<'a, E>(exec: E, batch_id: &str) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE expand_batch 
            SET 
                notified_complete = 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE batch_id = ?
                AND status = 1
        "#;

        sqlx::query(sql)
            .bind(batch_id)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 检查批次是否已通知后端完成
    pub async fn is_batch_notified<'a, E>(exec: E, batch_id: &str) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT notified_complete = 1 AS is_notified 
            FROM expand_batch 
            WHERE batch_id = ?
        "#;

        let is_notified: Option<bool> = sqlx::query_scalar(sql)
            .bind(batch_id)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(is_notified.unwrap_or(false))
    }

    pub async fn get_all_done_but_not_notified<'a, E>(
        exec: E,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        SELECT * FROM expand_batch
        WHERE status = 1 AND notified_complete = 0
    "#;

        sqlx::query_as::<sqlx::Sqlite, ExpandBatchEntity>(sql)
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
            WHERE status = 1 
                AND notified_complete = 0
                AND uid = ?
                AND chain_code = ?
        "#;

        sqlx::query_as::<sqlx::Sqlite, ExpandBatchEntity>(sql)
            .bind(uid)
            .bind(chain_code)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
