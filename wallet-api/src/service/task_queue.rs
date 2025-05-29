use wallet_database::repositories::ResourcesRepo;

use crate::response_vo::task_queue::TaskQueueStatus;

pub struct TaskQueueService {
    repo: ResourcesRepo,
}

impl TaskQueueService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo }
    }

    pub async fn get_task_queue_status(self) -> Result<TaskQueueStatus, crate::ServiceError> {
        let mut repo = self.repo;
        use wallet_database::repositories::task_queue::TaskQueueRepoTrait as _;
        let all = repo.all_tasks_queue().await?;
        let done = repo.done_task_queue().await?;
        let running = repo.running_task_queue().await?;
        let pending = repo.pending_task_queue().await?;
        let failed = repo.failed_task_queue().await?;

        let status = TaskQueueStatus {
            all_tasks: all.len(),
            running_tasks: running.len(),
            pending_tasks: pending.len(),
            failed_tasks: failed.len(),
            done_tasks: done.len(),
        };

        tracing::info!(?status, "Current task queue status");

        Ok(status)
    }
}
