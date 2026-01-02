use wallet_database::{
    entities::expand_batch_item::ExpandItemStatus,
    repositories::{
        ResourcesRepo,
        api_wallet::{
            address_query_state::AddressQueryStateRepo, expand_batch::ExpandBatchRepo,
            expand_batch_item::ExpandBatchItemRepo,
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
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let all = TaskQueueRepo::all_tasks_queue(&pool).await?;
        let done = TaskQueueRepo::done_task_queue(&pool).await?;
        let running = TaskQueueRepo::running_task_queue(&pool).await?;
        let pending = TaskQueueRepo::pending_task_queue(&pool).await?;
        let failed_tasks_list = TaskQueueRepo::failed_task_queue(&pool).await?;

        let bill_count = repo.bill_count().await?;

        // 获取未完成的 expand_batch 数据
        let expand_batches = ExpandBatchRepo::get_unfinished_batches(pool.clone()).await?;

        // 获取所有 expand_batch_item 数据用于统计
        let all_batch_items = ExpandBatchItemRepo::get_all(pool.clone()).await?;

        // 统计各状态的数量
        let creating_items_count =
            all_batch_items.iter().filter(|item| item.status == ExpandItemStatus::Creating).count();
        let initing_items_count =
            all_batch_items.iter().filter(|item| item.status == ExpandItemStatus::Initing).count();
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
            creating_items_count,
            initing_items_count,
            done_items_count,
            failed_items_count,
        };

        tracing::info!(?status, "Current task queue status");

        Ok(status)
    }
}
