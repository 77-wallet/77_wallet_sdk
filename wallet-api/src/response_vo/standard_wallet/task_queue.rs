use serde::Serialize;
use wallet_database::entities::task_queue::TaskQueueEntity;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueueStatus {
    pub all_tasks: usize,
    pub running_tasks: usize,
    pub pending_tasks: usize,
    pub failed_tasks: usize,
    pub done_tasks: usize,
    pub bill_count: i64,
    pub failed_tasks_list: Vec<TaskQueueEntity>,
}
