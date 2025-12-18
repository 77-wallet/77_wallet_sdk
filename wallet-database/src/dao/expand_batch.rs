use sqlx::{Executor, Sqlite};

use crate::entities::expand_batch::{CreateExpandBatchEntity, ExpandBatchEntity};

pub struct ExpandBatchDao {}

impl ExpandBatchDao {
    /// 创建新的扩容批次
    pub async fn create<'a, E>(
        exec: E,
        req: CreateExpandBatchEntity,
        current_time: i64,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO expand_batch 
            (batch_id, chain_code, total_count, finished_count, status, created_at, updated_at)
            VALUES (?, ?, ?, 0, 0, ?, ?)
            ON CONFLICT (batch_id) DO UPDATE SET 
                total_count = MAX(total_count, excluded.total_count),
                updated_at = excluded.updated_at
        "#;

        sqlx::query(sql)
            .bind(&req.batch_id)
            .bind(&req.chain_code)
            .bind(req.total_count)
            .bind(current_time)
            .bind(current_time)
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
                finished_count = finished_count + ?,
                updated_at = strftime('%s', 'now')
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
    pub async fn mark_as_done<'a, E>(exec: E, batch_id: &str) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE expand_batch 
            SET 
                status = 1,
                finished_count = total_count,
                updated_at = strftime('%s', 'now')
            WHERE batch_id = ?
                AND status = 0
                AND finished_count >= total_count
        "#;

        sqlx::query(sql)
            .bind(batch_id)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
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
}
