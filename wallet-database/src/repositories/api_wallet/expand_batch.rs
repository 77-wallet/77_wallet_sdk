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

    /// 标记批次已通知后端完成
    pub async fn mark_as_notified(pool: DbPool, batch_id: &str) -> Result<bool, crate::Error> {
        ExpandBatchDao::update_status(
            pool.as_ref(),
            batch_id,
            ExpandBatchStatus::Done,
            ExpandBatchStatus::Notified,
        )
        .await
    }

    /// 标记批次为完成（如果已完成）
    pub async fn mark_done_if_finished(pool: DbPool, batch_id: &str) -> Result<bool, crate::Error> {
        ExpandBatchDao::mark_done_if_finished(pool.as_ref(), batch_id).await
    }

    /// 重新计算批次已完成计数
    pub async fn recompute_finished_count(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<bool, crate::Error> {
        ExpandBatchDao::recompute_finished_count(pool.as_ref(), uid, chain_code).await
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
        ExpandBatchDao::get_by_status(pool.as_ref(), ExpandBatchStatus::Done).await
    }

    /// 找出所有未完成的 batch（finished < total）
    pub async fn get_unfinished_batches(
        pool: DbPool,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_unfinished_batches(pool.as_ref()).await
    }

    /// 获取所有批次
    pub async fn get_all(
        pool: DbPool,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_all(pool.as_ref()).await
    }
}
