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

    pub async fn mark_item_status(
        pool: DbPool,
        batch_id: &str,
        input_index: i32,
        status: ExpandItemStatus,
    ) -> Result<(), crate::Error> {
        ExpandBatchItemDao::mark_item_status_by_batch(pool.as_ref(), batch_id, input_index, status)
            .await
    }

    /// 更新单个扩容项状态为完成
    pub async fn mark_item_done(
        pool: DbPool,
        batch_id: &str,
        input_index: i32,
    ) -> Result<(), crate::Error> {
        ExpandBatchItemDao::mark_item_status_by_batch(
            pool.as_ref(),
            batch_id,
            input_index,
            ExpandItemStatus::Done,
        )
        .await
    }

    pub async fn mark_items_done_by_owner(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
        input_indices: &[i32],
    ) -> Result<u64, crate::Error> {
        ExpandBatchItemDao::mark_items_status_by_owner_from(
            pool.as_ref(),
            uid,
            chain_code,
            input_indices,
            ExpandItemStatus::Initing,
            ExpandItemStatus::Done,
        )
        .await
    }

    /// 批量更新扩容项状态
    pub async fn mark_items_status_from(
        pool: DbPool,
        batch_id: &str,
        input_indices: &[i32],
        from: ExpandItemStatus,
        to: ExpandItemStatus,
    ) -> Result<u64, crate::Error> {
        ExpandBatchItemDao::mark_items_status_by_batch_from(
            pool.as_ref(),
            batch_id,
            input_indices,
            from,
            to,
        )
        .await
    }

    pub async fn rollback_status(
        pool: DbPool,
        batch_id: &str,
        input_indices: &[i32],
        from: ExpandItemStatus,
        to: ExpandItemStatus,
    ) -> Result<u64, crate::Error> {
        ExpandBatchItemDao::mark_items_status_by_batch_from(
            pool.as_ref(),
            batch_id,
            input_indices,
            from,
            to,
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

    /// 获取批次的所有扩容项
    pub async fn fetch_pending(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
        limit: i64,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error> {
        ExpandBatchItemDao::fetch_pending(pool.as_ref(), uid, chain_code, limit).await
    }

    /// 将所有未完成的 item 重置为 Pending（用于 recover）
    pub async fn reset_unfinished_to_pending(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<u64, crate::Error> {
        ExpandBatchItemDao::reset_unfinished_to_pending(pool.as_ref(), uid, chain_code).await
    }
}
