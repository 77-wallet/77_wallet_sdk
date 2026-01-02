use crate::{
    DbPool,
    dao::expand_batch_item::ExpandBatchItemDao,
    entities::expand_batch_item::{
        CreateExpandBatchItemEntity, ExpandBatchItemEntity, ExpandItemStatus,
    },
};

pub struct ExpandBatchItemRepo;

impl ExpandBatchItemRepo {
    /// 批量创建扩容项
    pub async fn batch_create_items(
        pool: DbPool,
        uid: &str,
        batch_id: &str,
        chain_code: &str,
        input_indices: &[i32],
    ) -> Result<(), crate::Error> {
        let items: Vec<CreateExpandBatchItemEntity> = input_indices
            .iter()
            .map(|&index| CreateExpandBatchItemEntity::new(batch_id, uid, chain_code, index))
            .collect();
        ExpandBatchItemDao::batch_create(pool.as_ref(), items).await
    }

    /// 将状态从 Creating 推进到 Initing
    pub async fn creating_to_initing_if_match(
        pool: DbPool,
        batch_id: &str,
        input_indices: &[i32],
    ) -> Result<u64, crate::Error> {
        ExpandBatchItemDao::mark_items_status_by_batch_from(
            pool.as_ref(),
            batch_id,
            input_indices,
            ExpandItemStatus::Creating,
            ExpandItemStatus::Initing,
        )
        .await
    }

    /// 将状态从 Initing 推进到 Done
    pub async fn initing_to_done_if_match(
        pool: DbPool,
        batch_id: &str,
        input_indices: &[i32],
    ) -> Result<u64, crate::Error> {
        ExpandBatchItemDao::mark_items_status_by_batch_from(
            pool.as_ref(),
            batch_id,
            input_indices,
            ExpandItemStatus::Initing,
            ExpandItemStatus::Done,
        )
        .await
    }

    /// 根据状态获取批次的所有扩容项
    pub async fn fetch_by_status(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
        status: ExpandItemStatus,
        limit: i64,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error> {
        ExpandBatchItemDao::fetch_by_status(pool.as_ref(), uid, chain_code, status, limit).await
    }

    /// 统计 inflight 状态的扩容项数量
    pub async fn count_inflight(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<i64, crate::Error> {
        ExpandBatchItemDao::count_inflight(pool.as_ref(), uid, chain_code).await
    }

    /// 根据索引列表获取批次的所有扩容项状态
    pub async fn list_status_by_indices(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
        input_indices: &[i32],
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error> {
        ExpandBatchItemDao::list_status_by_indices(pool.as_ref(), uid, chain_code, input_indices)
            .await
    }

    /// 获取批次的所有扩容项
    pub async fn get_items_by_batch_id(
        pool: DbPool,
        batch_id: &str,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error> {
        ExpandBatchItemDao::get_items_by_batch_id(pool.as_ref(), batch_id).await
    }

    /// 检查某个批次的所有扩容项是否都已完成
    pub async fn is_batch_all_done(pool: DbPool, batch_id: &str) -> Result<bool, crate::Error> {
        ExpandBatchItemDao::is_batch_all_done(pool.as_ref(), batch_id).await
    }

    /// 获取批次的完成进度
    pub async fn get_batch_progress(
        pool: DbPool,
        batch_id: &str,
    ) -> Result<(i32, i32), crate::Error> {
        ExpandBatchItemDao::get_batch_progress(pool.as_ref(), batch_id).await
    }

    pub async fn find_batches_by_indices(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
        indices: &[i32],
    ) -> Result<Vec<(String, i64)>, crate::Error> {
        ExpandBatchItemDao::find_batches_by_indices(pool.as_ref(), uid, chain_code, indices).await
    }

    /// 根据批次 ID 和状态获取扩容项
    pub async fn fetch_by_batch_and_status(
        pool: DbPool,
        batch_id: &str,
        status: ExpandItemStatus,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error> {
        ExpandBatchItemDao::fetch_by_batch_and_status(pool.as_ref(), batch_id, status).await
    }

    /// 获取重试中的扩容项
    pub async fn fetch_retryable(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
        limit: i64,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error> {
        ExpandBatchItemDao::fetch_retryable(pool.as_ref(), uid, chain_code, limit).await
    }

    /// 批量更新扩容项状态
    pub async fn mark_failed_and_inc_retry(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
        input_indices: &[i32],
        phase: ExpandItemStatus,
    ) -> Result<u64, crate::Error> {
        ExpandBatchItemDao::mark_failed_and_inc_retry(
            pool.as_ref(),
            uid,
            chain_code,
            input_indices,
            phase,
        )
        .await
    }

    /// 将所有未完成的 item 重置为 Creating（用于 recover）
    ///
    /// 注意：不再重置为 Pending 状态，因为 Item 现在直接被创建为 Creating 状态
    pub async fn reset_unfinished_to_creating(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<u64, crate::Error> {
        ExpandBatchItemDao::reset_unfinished_to_creating(pool.as_ref(), uid, chain_code).await
    }

    /// 获取当前 uid + chain 下，所有已占用的 input_index
    ///
    /// 包括所有 batch / 所有状态：
    /// Pending / Creating / Initing / Failed / Done
    ///
    /// 用于 index 分配 & recover 逻辑
    pub async fn get_all_used_indices(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<Vec<i32>, crate::Error> {
        ExpandBatchItemDao::get_all_occupied_indices(pool.as_ref(), uid, chain_code).await
    }

    /// 统计批次下的扩容项数量
    pub async fn count_by_batch_id(pool: DbPool, batch_id: &str) -> Result<i64, crate::Error> {
        ExpandBatchItemDao::count_by_batch_id(pool.as_ref(), batch_id).await
    }

    /// 统计所有扩容项数量
    pub async fn count_all(pool: DbPool) -> Result<i64, crate::Error> {
        ExpandBatchItemDao::count_all(pool.as_ref()).await
    }

    /// 统计特定状态的扩容项数量
    pub async fn count_by_status(
        pool: DbPool,
        status: ExpandItemStatus,
    ) -> Result<i64, crate::Error> {
        ExpandBatchItemDao::count_by_status(pool.as_ref(), status).await
    }

    /// 获取所有扩容项
    pub async fn get_all(pool: DbPool) -> Result<Vec<ExpandBatchItemEntity>, crate::Error> {
        ExpandBatchItemDao::get_all(pool.as_ref()).await
    }

    /// 获取需要扫描的items
    pub async fn get_items_for_scan(
        pool: DbPool,
        batch_id: &str,
        limit: i64,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error> {
        ExpandBatchItemDao::get_items_for_scan(pool.as_ref(), batch_id, limit).await
    }

    /// 统计批次下的done状态item数量
    pub async fn count_done_items(pool: DbPool, batch_id: &str) -> Result<i64, crate::Error> {
        ExpandBatchItemDao::count_done_items(pool.as_ref(), batch_id).await
    }
}
