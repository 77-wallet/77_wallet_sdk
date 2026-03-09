use crate::{
    ApiWalletDbPool,
    dao::expand_batch::{CreateExpandBatchDao, ExpandBatchDao},
    entities::expand_batch::{BatchWithCount, ExpandBatchEntity, ExpandBatchStatus},
};

pub struct ExpandBatchRepo;

impl ExpandBatchRepo {
    /// 创建新的扩容批次
    pub async fn create_batch(
        pool: &ApiWalletDbPool,
        uid: &str,
        batch_id: &str,
        serial_no: &str,
        chain_code: &str,
        total_count: i32,
    ) -> Result<(), crate::Error> {
        let create_entity =
            CreateExpandBatchDao::new(uid, batch_id, serial_no, chain_code, total_count);

        ExpandBatchDao::create(pool.as_ref(), create_entity).await
    }

    /// 获取批次信息
    pub async fn get_batch(
        pool: &ApiWalletDbPool,
        batch_id: &str,
    ) -> Result<Option<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_batch(pool.as_ref(), batch_id).await
    }

    /// 获取运行中的批次中，item 数量不匹配的批次（仅用于修复、debug和离线校验）
    ///
    /// ⚠️ 重要警告：
    /// - 此方法仅用于**数据修复**、**debug**和**离线校验**
    /// - **严禁用于状态推进或业务决策**
    /// - 违反此规则将导致事实驱动架构失效
    pub async fn get_running_batches_item_count_mismatch_for_repair(
        pool: &ApiWalletDbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<Vec<BatchWithCount>, crate::Error> {
        ExpandBatchDao::get_running_batches_item_count_mismatch_for_repair(
            pool.as_ref(),
            uid,
            chain_code,
        )
        .await
    }

    /// 检查批次是否已通知后端完成（基于事实驱动）
    ///
    /// 事实驱动的判断：
    /// - 仅检查 `expand_complete_at` 字段
    /// - 该字段是不可逆的事实，一旦设置就永远不会改变
    /// - 表示批次已成功通知后端完成
    pub async fn is_batch_notified_fact(
        pool: &ApiWalletDbPool,
        batch_id: &str,
    ) -> Result<bool, crate::Error> {
        ExpandBatchDao::is_batch_notified_fact(pool.as_ref(), batch_id).await
    }

    /// 获取所有已完成但未通知后端的批次
    pub async fn get_all_finished_but_running(
        pool: &ApiWalletDbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_all_finished_but_running(pool.as_ref(), uid, chain_code).await
    }

    /// 获取批次的完成进度
    pub async fn get_batch_progress(
        pool: &ApiWalletDbPool,
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
        pool: &ApiWalletDbPool,
        batch_id: &str,
    ) -> Result<bool, crate::Error> {
        ExpandBatchDao::done_to_notified_if_match(pool.as_ref(), batch_id).await
    }

    /// 更新expand_complete_at字段，仅当它为NULL时
    pub async fn update_expand_complete_at_if_null(
        pool: &ApiWalletDbPool,
        batch_id: &str,
    ) -> Result<bool, crate::Error> {
        ExpandBatchDao::update_expand_complete_at_if_null(pool.as_ref(), batch_id).await
    }

    /// 更新批次的finished_count缓存值
    ///
    /// ⚠️ 重要注意事项：
    /// - 此方法仅用于更新finished_count缓存值，不得用于业务决策或状态推进
    /// - finished_count是缓存值，不是事实，不参与任何业务判断
    /// - 唯一的完成事实是local_complete_at字段
    pub async fn update_finished_count_cache_only(
        pool: &ApiWalletDbPool,
        batch_id: &str,
        count: i64,
    ) -> Result<bool, crate::Error> {
        ExpandBatchDao::update_finished_count_cache_only(pool.as_ref(), batch_id, count).await
    }

    /// 获取所有运行中的批次
    pub async fn get_all_running_batches(
        pool: &ApiWalletDbPool,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_by_status(pool.as_ref(), ExpandBatchStatus::Running).await
    }

    /// 获取需要进行item reconciliation的批次（事实驱动）
    ///
    /// 批次满足：
    /// - 状态为 Running 但 local_complete_at 已设置（需要推进到 Done）
    ///
    /// 用于 Scanner 的 item reconciliation 逻辑
    pub async fn get_batches_for_item_reconcile(
        pool: &ApiWalletDbPool,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_batches_for_item_reconcile(pool.as_ref()).await
    }

    /// 获取需要通知的批次（事实驱动）
    ///
    /// 批次满足：
    /// - 状态为 Done 但 expand_complete_at 未设置（需要推进到 Notified）
    ///
    /// 用于 Scanner 的通知逻辑
    pub async fn get_batches_for_notify(
        pool: &ApiWalletDbPool,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_batches_for_notify(pool.as_ref()).await
    }

    /// 检查批次的通知状态与事实是否一致
    ///
    /// 状态一致性检查：
    /// - 检查 status 是否为 Notified 且 expand_complete_at 事实存在
    /// - 用于验证状态与底层事实的一致性
    /// - 此方法**不是**事实判断，仅用于状态验证
    pub async fn is_batch_notified_state_consistent(
        pool: &ApiWalletDbPool,
        batch_id: &str,
    ) -> Result<bool, crate::Error> {
        ExpandBatchDao::is_batch_notified_state_consistent(pool.as_ref(), batch_id).await
    }

    /// 获取已完成但未通知后端的批次
    pub async fn get_done_but_not_notified(
        pool: &ApiWalletDbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_done_but_not_notified(pool.as_ref(), uid, chain_code).await
    }

    /// 获取所有已完成但未通知后端的批次
    pub async fn get_all_done_but_not_notified(
        pool: &ApiWalletDbPool,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        let sql = r#"
            SELECT * FROM expand_batch 
            WHERE status = ? 
                AND expand_complete_at IS NULL
        "#;

        sqlx::query_as::<sqlx::Sqlite, ExpandBatchEntity>(sql)
            .bind(ExpandBatchStatus::Done)
            .fetch_all(pool.as_ref())
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 获取所有已完成的批次
    pub async fn get_all_done(
        pool: &ApiWalletDbPool,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_all_done(pool.as_ref()).await
    }

    /// 找出所有未完成的 batch（finished < total）
    pub async fn get_unfinished_batches(
        pool: &ApiWalletDbPool,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_unfinished_batches(pool.as_ref()).await
    }

    /// 获取所有批次
    pub async fn get_all(pool: &ApiWalletDbPool) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_all(pool.as_ref()).await
    }

    /// 获取指定状态的批次
    pub async fn get_by_status(
        pool: &ApiWalletDbPool,
        status: ExpandBatchStatus,
    ) -> Result<Vec<ExpandBatchEntity>, crate::Error> {
        ExpandBatchDao::get_by_status(pool.as_ref(), status).await
    }

    /// 将批次状态从Pending转为Running，使用CAS确保只有一个实例能成功
    pub async fn mark_running_if_pending(
        pool: &ApiWalletDbPool,
        batch_id: &str,
    ) -> Result<bool, crate::Error> {
        ExpandBatchDao::mark_running_if_pending(pool.as_ref(), batch_id).await
    }

    /// 当所有扩容项都已完成时，标记本地扩容完成
    ///
    /// 事实驱动的本地完成确认：
    /// - 仅当所有 items 都已完成（status = Done）时设置 local_complete_at
    /// - 使用CAS确保只有第一个调用者能成功写入
    /// - local_complete_at 是不可逆事实，一旦设置就永远不会改变
    pub async fn mark_local_complete_if_all_items_done(
        pool: &ApiWalletDbPool,
        batch_id: &str,
    ) -> Result<u64, crate::Error> {
        ExpandBatchDao::mark_local_complete_if_all_items_done(pool.as_ref(), batch_id).await
    }

    /// 基于本地完成事实推进批次状态：当local_complete_at已设置但状态仍为Running时，推进到Done
    ///
    /// 事实驱动的状态追平：
    /// - 仅当 local_complete_at IS NOT NULL 且 status = Running 时推进
    /// - 使用CAS确保并发安全
    /// - 返回影响行数，便于上层日志区分状态
    pub async fn mark_done_if_local_completed(
        pool: &ApiWalletDbPool,
        batch_id: &str,
    ) -> Result<u64, crate::Error> {
        ExpandBatchDao::mark_done_if_local_completed(pool.as_ref(), batch_id).await
    }

    /// 检查批次的本地扩容是否已完成（基于事实驱动）
    ///
    /// 注意：这是事实驱动的完成判断，仅检查 `local_complete_at` 字段
    /// 该字段是不可逆的事实，一旦设置就永远不会改变
    pub async fn is_local_completed(
        pool: &ApiWalletDbPool,
        batch_id: &str,
    ) -> Result<bool, crate::Error> {
        let batch = match Self::get_batch(pool, batch_id).await? {
            Some(batch) => batch,
            None => return Ok(false),
        };
        Ok(batch.local_complete_at.is_some())
    }
}
