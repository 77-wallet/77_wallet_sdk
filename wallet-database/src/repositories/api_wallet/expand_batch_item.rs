use crate::{
    DbPool,
    dao::expand_batch_item::ExpandBatchItemDao,
    entities::expand_batch_item::{CreateExpandBatchItemEntity, ExpandBatchItemEntity},
};

pub struct ExpandBatchItemRepo;

impl ExpandBatchItemRepo {
    /// 批量创建扩容项
    pub async fn batch_create_items(
        pool: &DbPool,
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

    /// 更新单个扩容项状态为完成
    pub async fn mark_item_done(
        pool: &DbPool,
        batch_id: &str,
        input_index: i32,
    ) -> Result<(), crate::Error> {
        ExpandBatchItemDao::mark_item_done(pool.as_ref(), batch_id, input_index).await
    }

    /// 获取批次的所有扩容项
    pub async fn get_items_by_batch_id(
        pool: &DbPool,
        batch_id: &str,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error> {
        ExpandBatchItemDao::get_items_by_batch_id(pool.as_ref(), batch_id).await
    }

    /// 检查某个批次的所有扩容项是否都已完成
    pub async fn is_batch_all_done(pool: &DbPool, batch_id: &str) -> Result<bool, crate::Error> {
        ExpandBatchItemDao::is_batch_all_done(pool.as_ref(), batch_id).await
    }

    /// 获取批次的完成进度
    pub async fn get_batch_progress(
        pool: &DbPool,
        batch_id: &str,
    ) -> Result<(i32, i32), crate::Error> {
        ExpandBatchItemDao::get_batch_progress(pool.as_ref(), batch_id).await
    }

    pub async fn find_batches_by_indices(
        pool: &DbPool,
        uid: &str,
        chain_code: &str,
        indices: &[i32],
    ) -> Result<Vec<(String, i64)>, crate::Error> {
        ExpandBatchItemDao::find_batches_by_indices(pool.as_ref(), uid, chain_code, indices).await
    }
}
