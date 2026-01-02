use crate::{
    DbPool,
    dao::expand_batch::ExpandBatchDao,
    entities::expand_batch::{
        BatchWithCount, CreateExpandBatchEntity, ExpandBatchEntity, ExpandBatchStatus,
    },
};

pub struct ExpandBatchRepo;

impl ExpandBatchRepo {
    /// 创建新的扩容批次
    pub async fn create_batch(
        pool: DbPool,
        uid: &str,
        batch_id: &str,
        serial_no: &str,
        chain_code: &str,
        total_count: i32,
    ) -> Result<(), crate::Error> {
        let create_entity =
            CreateExpandBatchEntity::new(uid, batch_id, serial_no, chain_code, total_count);

        ExpandBatchDao::create(pool.as_ref(), create_entity).await
    }

    /// 获取批次信息
    pub async fn get_batch(
        pool: DbPool,
        batch_id: &str,
    ) -> Result<Option<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_batch(pool.as_ref(), batch_id).await
    }

    /// 获取运行中的批次中，item 数量不足的批次
    pub async fn get_running_batches_with_insufficient_items(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<Vec<BatchWithCount>, crate::Error> {
        ExpandBatchDao::get_running_batches_with_insufficient_items(pool.as_ref(), uid, chain_code)
            .await
    }

    /// 检查批次是否已完成
    pub async fn is_batch_done(pool: DbPool, batch_id: &str) -> Result<bool, crate::Error> {
        ExpandBatchDao::is_batch_done(pool.as_ref(), batch_id).await
    }

    /// 获取所有已完成但未通知后端的批次
    pub async fn get_all_finished_but_running(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_all_finished_but_running(pool.as_ref(), uid, chain_code).await
    }

    /// 获取批次的完成进度
    pub async fn get_batch_progress(
        pool: DbPool,
        batch_id: &str,
    ) -> Result<Option<(i32, i32)>, crate::Error> {
        if let Some(batch) = Self::get_batch(pool, batch_id).await? {
            Ok(Some((batch.total_count, batch.finished_count)))
        } else {
            Ok(None)
        }
    }

    /// 将状态从 Done 推进到 Notified
    pub async fn done_to_notified_if_match(
        pool: DbPool,
        batch_id: &str,
    ) -> Result<bool, crate::Error> {
        ExpandBatchDao::done_to_notified_if_match(pool.as_ref(), batch_id).await
    }

    /// 更新expand_complete_at字段，仅当它为NULL时
    pub async fn update_expand_complete_at_if_null(
        pool: DbPool,
        batch_id: &str,
    ) -> Result<bool, crate::Error> {
        ExpandBatchDao::update_expand_complete_at_if_null(pool.as_ref(), batch_id).await
    }

    /// 标记批次为完成（如果已完成）
    pub async fn mark_done_if_finished(pool: DbPool, batch_id: &str) -> Result<bool, crate::Error> {
        ExpandBatchDao::mark_done_if_finished(pool.as_ref(), batch_id).await
    }

    /// 更新批次的finished_count缓存
    pub async fn update_finished_count(
        pool: DbPool,
        batch_id: &str,
        count: i64,
    ) -> Result<bool, crate::Error> {
        ExpandBatchDao::update_finished_count(pool.as_ref(), batch_id, count).await
    }

    /// 标记批次为Done
    pub async fn mark_as_done(pool: DbPool, batch_id: &str) -> Result<bool, crate::Error> {
        ExpandBatchDao::update_status(
            pool.as_ref(),
            batch_id,
            ExpandBatchStatus::Running,
            ExpandBatchStatus::Done,
        )
        .await
    }

    /// 获取所有运行中的批次
    pub async fn get_all_running_batches(
        pool: DbPool,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_by_status(pool.as_ref(), ExpandBatchStatus::Running).await
    }

    /// 检查批次是否已通知后端完成
    pub async fn is_batch_notified(pool: DbPool, batch_id: &str) -> Result<bool, crate::Error> {
        ExpandBatchDao::is_batch_notified(pool.as_ref(), batch_id).await
    }

    /// 获取已完成但未通知后端的批次
    pub async fn get_done_but_not_notified(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_done_but_not_notified(pool.as_ref(), uid, chain_code).await
    }

    /// 获取所有已完成但未通知后端的批次
    pub async fn get_all_done_but_not_notified(
        pool: DbPool,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        let sql = r#"
            SELECT * FROM expand_batch 
            WHERE status = ? 
                AND expand_complete_at IS NOT NULL
        "#;

        sqlx::query_as::<sqlx::Sqlite, ExpandBatchEntity>(sql)
            .bind(ExpandBatchStatus::Done)
            .fetch_all(pool.as_ref())
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 找出所有未完成的 batch（finished < total）
    pub async fn get_unfinished_batches(
        pool: DbPool,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_unfinished_batches(pool.as_ref()).await
    }

    /// 获取所有批次
    pub async fn get_all(pool: DbPool) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_all(pool.as_ref()).await
    }

    /// 获取指定状态的批次
    pub async fn get_by_status(
        pool: DbPool,
        status: ExpandBatchStatus,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_by_status(pool.as_ref(), status).await
    }

    /// 将批次状态从Pending转为Running，使用CAS确保只有一个实例能成功
    pub async fn mark_running_if_pending(
        pool: DbPool,
        batch_id: &str,
    ) -> Result<bool, crate::Error> {
        ExpandBatchDao::mark_running_if_pending(pool.as_ref(), batch_id).await
    }
}
