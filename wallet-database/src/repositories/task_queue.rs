use crate::{
    DbPool,
    dao::task_queue::TaskQueueDao,
    entities::task_queue::{CreateTaskQueueEntity, TaskName, TaskQueueEntity},
};

pub struct TaskQueueRepo {}

impl TaskQueueRepo {
    pub async fn create_multi_task(
        pool: &DbPool,
        req: &[CreateTaskQueueEntity],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::upsert_multi_task(pool.as_ref(), req).await?)
    }

    pub async fn create_task(
        pool: &DbPool,
        req: CreateTaskQueueEntity,
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::upsert(pool.as_ref(), req).await?)
    }

    pub async fn all_tasks_queue(pool: &DbPool) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), None, None).await?)
    }

    pub async fn task_failed(
        pool: &DbPool,
        id: &str,
        err_msg: &str,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::task_failed(pool.as_ref(), id, 3, err_msg).await?)
    }

    pub async fn task_detail(
        pool: &DbPool,
        id: &str,
    ) -> Result<Option<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::get_task_queue(pool.as_ref(), id).await?)
    }

    pub async fn get_task_with_task_name(
        pool: &DbPool,
        task_name: TaskName,
        status: &[u8],
    ) -> Result<Option<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::get_task_with_task_name(pool.as_ref(), task_name, status).await?)
    }

    pub async fn list_tasks_with_task_name(
        pool: &DbPool,
        task_name: TaskName,
        status: &[u8],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list_tasks_with_task_name(pool.as_ref(), task_name, status).await?)
    }

    pub async fn get_tasks_with_request_body(
        pool: &DbPool,
        keyword: &str,
        status: &[u8],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::get_tasks_with_request_body(pool.as_ref(), keyword, status).await?)
    }

    pub async fn get_tasks_with_request_body_and_task_name(
        pool: &DbPool,
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
        pool: &DbPool,
        id: &str,
        remark: &str,
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::update_task_remark(pool.as_ref(), id, remark).await?)
    }

    pub async fn delete_task(pool: &DbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete(pool.as_ref(), id).await?)
    }

    pub async fn done_task_queue(pool: &DbPool) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), Some(2), None).await?)
    }

    pub async fn failed_task_queue(pool: &DbPool) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), Some(3), None).await?)
    }

    pub async fn failed_mqtt_task_queue(
        pool: &DbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), Some(3), Some(2)).await?)
    }

    pub async fn running_task_queue(pool: &DbPool) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), Some(1), None).await?)
    }

    pub async fn hanging_task_queue(pool: &DbPool) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), Some(4), None).await?)
    }

    pub async fn pending_task_queue(pool: &DbPool) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.as_ref(), Some(0), None).await?)
    }

    pub async fn task_running(pool: &DbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::update_status(pool.as_ref(), id, 1).await?)
    }

    pub async fn task_done(pool: &DbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::update_status(pool.as_ref(), id, 2).await?)
    }

    pub async fn task_hang_up(pool: &DbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::update_status(pool.as_ref(), id, 4).await?)
    }

    pub async fn increase_retry_times(pool: &DbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::increase_retry_times(pool.as_ref(), id).await?)
    }

    pub async fn delete_old(pool: &DbPool, day: u16) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete_old(pool.as_ref(), day).await?)
    }

    pub async fn delete_oldest_by_status_when_exceeded(
        pool: &DbPool,
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

    pub async fn delete_all(pool: &DbPool, typ: Option<u8>) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete_all(pool.as_ref(), typ).await?)
    }

    pub async fn has_unfinished_task(pool: &DbPool) -> Result<bool, crate::Error> {
        Ok(TaskQueueDao::has_unfinished_task(pool.as_ref()).await?)
    }

    pub async fn delete_tasks_with_request_body_like(
        pool: &DbPool,
        keyword: &str,
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete_tasks_with_request_body_like(pool.as_ref(), keyword).await?)
    }
}
