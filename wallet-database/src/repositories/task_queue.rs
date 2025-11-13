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
}

#[async_trait::async_trait]
pub trait TaskQueueRepoTrait: super::TransactionTrait {
    // async fn create_multi_task(
    //     &mut self,
    //     req: &[CreateTaskQueueEntity],
    // ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
    //     let executor = self.get_conn_or_tx()?;
    //     crate::execute_with_executor!(executor, TaskQueueEntity::upsert_multi_task, req)
    // }

    // async fn create_task(&mut self, req: CreateTaskQueueEntity) -> Result<(), crate::Error> {
    //     let executor = self.get_conn_or_tx()?;
    //     crate::execute_with_executor!(executor, TaskQueueEntity::upsert, req)
    // }

    // async fn all_tasks_queue(&mut self) -> Result<Vec<TaskQueueEntity>, crate::Error> {
    //     let executor = self.get_conn_or_tx()?;
    //     crate::execute_with_executor!(executor, TaskQueueEntity::list, None, None)
    // }

    async fn done_task_queue(&mut self) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::list, Some(2), None)
    }

    async fn failed_task_queue(&mut self) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::list, Some(3), None)
    }

    async fn failed_mqtt_task_queue(&mut self) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::list, Some(3), Some(2))
    }

    async fn running_task_queue(&mut self) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::list, Some(1), None)
    }

    async fn hanging_task_queue(&mut self) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::list, Some(4), None)
    }

    async fn pending_task_queue(&mut self) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::list, Some(0), None)
    }

    async fn task_running(&mut self, id: &str) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::update_status, id, 1)
    }

    async fn task_done(&mut self, id: &str) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::update_status, id, 2)
    }

    async fn task_hang_up(&mut self, id: &str) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::update_status, id, 4)
    }

    async fn get_tasks_with_request_body(
        &mut self,
        request_body: &str,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            TaskQueueDao::get_tasks_with_request_body,
            request_body
        )
    }

    async fn increase_retry_times(&mut self, id: &str) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::increase_retry_times, id)
    }

    async fn delete_old(&mut self, day: u16) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::delete_old, day)
    }

    async fn delete_oldest_by_status_when_exceeded(
        &mut self,
        max_size: u32,
        target_status: u8,
    ) -> Result<(), crate::Error> {
        let executor = self.get_db_pool();
        TaskQueueDao::delete_oldest_by_status_when_exceeded(
            executor.as_ref(),
            max_size,
            target_status,
        )
        .await?;
        Ok(())
        // crate::execute_with_executor!(
        //     executor,
        //     TaskQueueEntity::delete_oldest_by_status_when_exceeded,
        //     max_size,
        //     target_status
        // )
    }

    async fn delete_all(&mut self, typ: Option<u8>) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::delete_all, typ)
    }

    async fn has_unfinished_task(&mut self) -> Result<bool, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, TaskQueueDao::has_unfinished_task,)
    }

    async fn delete_tasks_with_request_body_like(
        &mut self,
        keyword: &str,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            TaskQueueDao::delete_tasks_with_request_body_like,
            keyword
        )
    }
}
