use wallet_database::{
    entities::expand_batch_item::ExpandItemStatus,
    repositories::{
        ResourcesRepo,
        api_wallet::{
            account::ApiAccountRepo, address_query_state::AddressQueryStateRepo,
            expand_batch::ExpandBatchRepo, expand_batch_item::ExpandBatchItemRepo,
            wallet::ApiWalletRepo,
        },
        bill::BillRepoTrait,
        task_queue::TaskQueueRepo,
    },
};

use crate::response_vo::standard_wallet::task_queue::TaskQueueStatus;

pub struct TaskQueueService {
    repo: ResourcesRepo,
}

impl TaskQueueService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo }
    }

    pub async fn get_task_queue_status(
        self,
    ) -> Result<TaskQueueStatus, crate::error::service::ServiceError> {
        let mut repo = self.repo;
        let task_pool = crate::context::CONTEXT.get().unwrap().task_pool()?;
        let core_pool = crate::context::CONTEXT.get().unwrap().core_pool()?;
        let all = TaskQueueRepo::all_tasks_queue(&task_pool).await?;
        let done = TaskQueueRepo::done_task_queue(&task_pool).await?;
        let running = TaskQueueRepo::running_task_queue(&task_pool).await?;
        let pending = TaskQueueRepo::pending_task_queue(&task_pool).await?;
        let failed_tasks_list = TaskQueueRepo::failed_task_queue(&task_pool).await?;

        let bill_count = repo.bill_count().await?;

        // 获取未完成的 expand_batch 数据
        let expand_batches = ExpandBatchRepo::get_unfinished_batches(&core_pool).await?;

        // 获取所有 expand_batch_item 数据用于统计
        let all_batch_items = ExpandBatchItemRepo::get_all(&core_pool).await?;

        // 统计各状态的数量
        let creating_items_count = all_batch_items
            .iter()
            .filter(|item| item.status == ExpandItemStatus::CreateDispatched)
            .count();
        let initing_items_count = all_batch_items
            .iter()
            .filter(|item| item.status == ExpandItemStatus::InitDispatched)
            .count();
        let done_items_count =
            all_batch_items.iter().filter(|item| item.status == ExpandItemStatus::Done).count();
        let failed_items_count =
            all_batch_items.iter().filter(|item| item.status == ExpandItemStatus::Failed).count();

        // 获取未完成的 expand_batch_item 数据（排除Done状态）
        let expand_batch_items: Vec<_> = all_batch_items
            .into_iter()
            .filter(|item| item.status != ExpandItemStatus::Done)
            .collect();

        // 获取 address_query_state 表所有数据
        let address_query_states = AddressQueryStateRepo::get_all(&core_pool).await?;

        // 聚合地址恢复进度
        let mut address_recovery_progress = Vec::new();
        let mut total_local = 0usize;
        let mut total_remote = 0usize;

        for state in &address_query_states {
            let uid = state.uid.clone();
            let chain_code = state.chain_code.clone();

            // 1. 从 uid 获取 wallet_address
            let wallet_opt = ApiWalletRepo::find_by_uid(&core_pool, &uid).await?;

            if let Some(wallet) = wallet_opt {
                // 2. count local addresses
                let local_count = ApiAccountRepo::count_by_wallet_address_v2(
                    &core_pool,
                    &wallet.address,
                    None,
                    Some(chain_code.clone()),
                )
                .await? as usize;

                // 3. 获取 total_remote
                let state_total_remote = state.total_remote as usize;
                let estimated_total = if state_total_remote == 0 {
                    // 如果 total_remote 为 0，使用 (last_page + 1) * 100 进行估算
                    ((state.last_page + 1) * 100) as usize
                } else {
                    state_total_remote
                };

                // 4. 计算进度
                let percent = if estimated_total == 0 {
                    0.0
                } else {
                    let p = local_count as f32 / estimated_total as f32;
                    p.min(1.0) // 进度不超过 100%
                };

                // 5. 判断是否完成
                let done = local_count >= state_total_remote && state_total_remote != 0;

                // 6. 添加到进度列表
                address_recovery_progress.push(
                    crate::response_vo::standard_wallet::task_queue::RecoveryProgress {
                        uid,
                        chain_code,
                        local_count,
                        total_remote: estimated_total,
                        percent,
                        done,
                    },
                );

                // 7. 累计总数用于计算总体进度
                total_local += local_count;
                total_remote += estimated_total;
            }
        }

        // 计算总体进度
        let overall_percent = if total_remote == 0 {
            0.0
        } else {
            let p = total_local as f32 / total_remote as f32;
            p.min(1.0) // 进度不超过 100%
        };

        let status = crate::response_vo::standard_wallet::task_queue::TaskQueueStatus {
            all_tasks: all.len(),
            running_tasks: running.len(),
            pending_tasks: pending.len(),
            failed_tasks: failed_tasks_list.len(),
            done_tasks: done.len(),
            bill_count,
            failed_tasks_list,
            expand_batches,
            expand_batch_items,
            address_query_states,
            creating_items_count,
            initing_items_count,
            done_items_count,
            failed_items_count,
            address_recovery_progress,
            overall_percent,
        };

        tracing::info!(?status, "Current task queue status");

        Ok(status)
    }
}
