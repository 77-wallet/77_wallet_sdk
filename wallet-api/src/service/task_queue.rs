use wallet_database::repositories::{ResourcesRepo, bill::BillRepoTrait};

use crate::response_vo::task_queue::TaskQueueStatus;

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
        use wallet_database::repositories::task_queue::TaskQueueRepoTrait as _;
        let all = repo.all_tasks_queue().await?;
        let done = repo.done_task_queue().await?;
        let running = repo.running_task_queue().await?;
        let pending = repo.pending_task_queue().await?;
        let failed_tasks_list = repo.failed_task_queue().await?;

        let bill_count = repo.bill_count().await?;

        let status = TaskQueueStatus {
            all_tasks: all.len(),
            running_tasks: running.len(),
            pending_tasks: pending.len(),
            failed_tasks: failed_tasks_list.len(),
            done_tasks: done.len(),
            bill_count,
            failed_tasks_list,
        };

        tracing::info!(?status, "Current task queue status");

        Ok(status)
    }
}
