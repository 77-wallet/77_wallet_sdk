use wallet_database::repositories::{
    ResourcesRepo, bill::BillRepoTrait, task_queue::TaskQueueRepo,
    api_wallet::{expand_batch::ExpandBatchRepo, expand_batch_item::ExpandBatchItemRepo, address_query_state::AddressQueryStateRepo},
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
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let all = TaskQueueRepo::all_tasks_queue(&pool).await?;
        let done = TaskQueueRepo::done_task_queue(&pool).await?;
        let running = TaskQueueRepo::running_task_queue(&pool).await?;
        let pending = TaskQueueRepo::pending_task_queue(&pool).await?;
        let failed_tasks_list = TaskQueueRepo::failed_task_queue(&pool).await?;

        let bill_count = repo.bill_count().await?;

        // 获取 expand_batch 表所有数据
        let expand_batches = ExpandBatchRepo::get_all(pool.clone()).await?;

        // 获取 expand_batch_item 表所有数据
        let expand_batch_items = ExpandBatchItemRepo::get_all(pool.clone()).await?;

        // 获取 address_query_state 表所有数据
        let address_query_states = AddressQueryStateRepo::get_all(&pool).await?;

        let status = TaskQueueStatus {
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
        };
        
        tracing::info!(?status, "Current task queue status");

        Ok(status)
    }
}
