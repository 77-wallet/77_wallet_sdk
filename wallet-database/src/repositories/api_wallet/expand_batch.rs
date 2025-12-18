use crate::{
    DbPool,
    dao::expand_batch::ExpandBatchDao,
    entities::expand_batch::{CreateExpandBatchEntity, ExpandBatchEntity},
};

pub struct ExpandBatchRepo;

impl ExpandBatchRepo {
    /// 创建新的扩容批次
    pub async fn create_batch(
        pool: &DbPool,
        batch_id: &str,
        chain_code: &str,
        total_count: i32,
    ) -> Result<(), crate::Error> {
        let create_entity = CreateExpandBatchEntity::new(batch_id, chain_code, total_count);
        let (create_entity, current_time) = create_entity.with_current_time();

        ExpandBatchDao::create(pool.as_ref(), create_entity, current_time).await
    }

    /// 原子增加已完成计数，支持一次性增加指定数量
    pub async fn increment_finished(
        pool: &DbPool,
        batch_id: &str,
        increment: usize,
    ) -> Result<(), crate::Error> {
        ExpandBatchDao::increment_finished(pool.as_ref(), batch_id, increment).await
    }

    /// 原子增加已完成计数，每次增加1个（兼容旧接口）
    pub async fn increment_finished_one(pool: &DbPool, batch_id: &str) -> Result<(), crate::Error> {
        ExpandBatchDao::increment_finished_one(pool.as_ref(), batch_id).await
    }

    /// 获取批次信息
    pub async fn get_batch(
        pool: &DbPool,
        batch_id: &str,
    ) -> Result<Option<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_batch(pool.as_ref(), batch_id).await
    }

    /// 检查批次是否已完成
    pub async fn is_batch_done(pool: &DbPool, batch_id: &str) -> Result<bool, crate::Error> {
        ExpandBatchDao::is_batch_done(pool.as_ref(), batch_id).await
    }

    /// 标记批次为完成
    pub async fn mark_as_done(pool: &DbPool, batch_id: &str) -> Result<(), crate::Error> {
        ExpandBatchDao::mark_as_done(pool.as_ref(), batch_id).await
    }

    /// 获取批次的完成进度
    pub async fn get_batch_progress(
        pool: &DbPool,
        batch_id: &str,
    ) -> Result<Option<(i32, i32)>, crate::Error> {
        if let Some(batch) = Self::get_batch(pool, batch_id).await? {
            Ok(Some((batch.total_count, batch.finished_count)))
        } else {
            Ok(None)
        }
    }
}
