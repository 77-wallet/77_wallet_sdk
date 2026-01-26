use crate::{
    CoreDbPool, TaskDbPool,
    dao::task_queue::TaskQueueDao,
    entities::task_queue::{CreateTaskQueueEntity, TaskName, TaskQueueEntity},
};

pub struct TaskQueueRepo {}

impl TaskQueueRepo {
    pub async fn create_multi_task(
        pool: &TaskDbPool,
        req: &[CreateTaskQueueEntity],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::upsert_multi_task(pool.as_ref(), req).await?)
    }

    pub async fn create_task(
        pool: &TaskDbPool,
        req: CreateTaskQueueEntity,
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::upsert(pool.as_ref(), req).await?)
    }

    pub async fn all_tasks_queue(pool: &TaskDbPool) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), None, None).await?)
    }

    pub async fn all_tasks_queue_core(
        pool: &CoreDbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), None, None).await?)
    }

    pub async fn task_failed(
        pool: &TaskDbPool,
        id: &str,
        err_msg: &str,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::task_failed(pool.as_ref(), id, 3, err_msg).await?)
    }

    pub async fn task_detail(
        pool: &TaskDbPool,
        id: &str,
    ) -> Result<Option<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::get_task_queue(pool.as_ref(), id).await?)
    }

    pub async fn get_task_with_task_name(
        pool: &TaskDbPool,
        task_name: TaskName,
        status: &[u8],
    ) -> Result<Option<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::get_task_with_task_name(pool.as_ref(), task_name, status).await?)
    }

    pub async fn list_tasks_with_task_name(
        pool: &TaskDbPool,
        task_name: TaskName,
        status: &[u8],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list_tasks_with_task_name(pool.as_ref(), task_name, status).await?)
    }

    pub async fn get_tasks_with_request_body(
        pool: &TaskDbPool,
        keyword: &str,
        status: &[u8],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::get_tasks_with_request_body(pool.as_ref(), keyword, status).await?)
    }

    pub async fn get_tasks_with_request_body_and_task_name(
        pool: &TaskDbPool,
        task_name: TaskName,
        keyword: &str,
        status: &[u8],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::get_tasks_with_request_body_and_task_name(
            pool.as_ref(),
            task_name,
            keyword,
            status,
        )
        .await?)
    }

    pub async fn update_task_remark(
        pool: &TaskDbPool,
        id: &str,
        remark: &str,
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::update_task_remark(pool.as_ref(), id, remark).await?)
    }

    pub async fn delete_task(pool: &TaskDbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete(pool.as_ref(), id).await?)
    }

    pub async fn done_task_queue(pool: &TaskDbPool) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), Some(2), None).await?)
    }

    pub async fn failed_task_queue(
        pool: &TaskDbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), Some(3), None).await?)
    }

    pub async fn failed_mqtt_task_queue(
        pool: &TaskDbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), Some(3), Some(2)).await?)
    }

    pub async fn running_task_queue(
        pool: &TaskDbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), Some(1), None).await?)
    }

    pub async fn hanging_task_queue(
        pool: &TaskDbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), Some(4), None).await?)
    }

    pub async fn pending_task_queue(
        pool: &TaskDbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), Some(0), None).await?)
    }

    pub async fn task_running(pool: &TaskDbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::update_status(pool.as_ref(), id, 1).await?)
    }

    pub async fn task_done(pool: &TaskDbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::update_status(pool.as_ref(), id, 2).await?)
    }

    pub async fn task_hang_up(pool: &TaskDbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::update_status(pool.as_ref(), id, 4).await?)
    }

    pub async fn increase_retry_times(pool: &TaskDbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::increase_retry_times(pool.as_ref(), id).await?)
    }

    pub async fn delete_old(pool: &TaskDbPool, day: u16) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete_old(pool.as_ref(), day).await?)
    }

    pub async fn delete_oldest_by_status_when_exceeded(
        pool: &TaskDbPool,
        max_size: u32,
        target_status: u8,
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete_oldest_by_status_when_exceeded(
            pool.as_ref(),
            max_size,
            target_status,
        )
        .await?)
    }

    pub async fn delete_all(pool: &TaskDbPool, typ: Option<u8>) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete_all(pool.as_ref(), typ).await?)
    }

    pub async fn has_unfinished_task(pool: &TaskDbPool) -> Result<bool, crate::Error> {
        Ok(TaskQueueDao::has_unfinished_task(pool.as_ref()).await?)
    }

    pub async fn delete_tasks_with_request_body_like(
        pool: &TaskDbPool,
        keyword: &str,
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete_tasks_with_request_body_like(pool.as_ref(), keyword).await?)
    }

    /// 批量插入TaskQueueEntity
    pub async fn insert_batch_task(
        pool: &TaskDbPool,
        tasks: &[TaskQueueEntity],
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::insert_batch(pool.as_ref(), tasks).await?)
    }

    /// 批量插入TaskQueueEntity，忽略冲突
    pub async fn insert_batch_task_ignore_conflict(
        pool: &TaskDbPool,
        tasks: &[TaskQueueEntity],
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::insert_batch_ignore_conflict(pool.as_ref(), tasks).await?)
    }

    /// 获取task_queue表记录数
    pub async fn count_tasks(pool: &TaskDbPool) -> Result<i64, crate::Error> {
        Ok(TaskQueueDao::count(pool.as_ref()).await?)
    }

    /// 获取core_db中task_queue表记录数
    pub async fn count_tasks_core(pool: &CoreDbPool) -> Result<i64, crate::Error> {
        Ok(TaskQueueDao::count(pool.as_ref()).await?)
    }

    /// 冻结旧表
    pub async fn freeze_table(pool: &TaskDbPool) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::freeze_table(pool.as_ref()).await?)
    }

    /// 冻结core_db中的旧表
    pub async fn freeze_table_core(pool: &CoreDbPool) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::freeze_table(pool.as_ref()).await?)
    }

    /// 检查表是否存在
    pub async fn table_exists(pool: &TaskDbPool, table_name: &str) -> Result<bool, crate::Error> {
        Ok(TaskQueueDao::table_exists(pool.as_ref(), table_name).await?)
    }

    /// 检查core_db中表是否存在
    pub async fn table_exists_core(
        pool: &CoreDbPool,
        table_name: &str,
    ) -> Result<bool, crate::Error> {
        Ok(TaskQueueDao::table_exists(pool.as_ref(), table_name).await?)
    }
}
