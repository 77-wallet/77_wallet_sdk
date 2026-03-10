use crate::{
    CoreDbPool, TaskDbPool,
    dao::task_queue::TaskQueueDao,
    entities::task_queue::{CreateTaskQueueEntity, TaskName, TaskQueueEntity},
};

pub struct TaskQueueRepo {}

impl TaskQueueRepo {
    pub fn build_backend_task(
        task_name: TaskName,
        request_body: Option<String>,
        remark: Option<String>,
    ) -> Result<CreateTaskQueueEntity, crate::Error> {
        CreateTaskQueueEntity::with_backend_request_string(task_name, request_body, remark)
    }

    pub fn build_mqtt_task(
        id: &str,
        task_name: TaskName,
        request_body: Option<String>,
        remark: Option<String>,
    ) -> Result<CreateTaskQueueEntity, crate::Error> {
        CreateTaskQueueEntity::with_mqtt_request_string(id, task_name, request_body, remark)
    }

    pub async fn create_multi_task(
        pool: &TaskDbPool,
        req: &[CreateTaskQueueEntity],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::upsert_multi_task(pool.write_ref(), req).await?)
    }

    pub async fn create_task(
        pool: &TaskDbPool,
        req: CreateTaskQueueEntity,
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::upsert(pool.write_ref(), req).await?)
    }

    pub async fn all_tasks_queue(pool: &TaskDbPool) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.read_ref(), None, None).await?)
    }

    pub async fn all_tasks_queue_core(
        pool: &CoreDbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.read_ref(), None, None).await?)
    }

    pub async fn task_failed(
        pool: &TaskDbPool,
        id: &str,
        err_msg: &str,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::task_failed(pool.write_ref(), id, 3, err_msg).await?)
    }

    pub async fn task_detail(
        pool: &TaskDbPool,
        id: &str,
    ) -> Result<Option<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::get_task_queue(pool.read_ref(), id).await?)
    }

    pub async fn get_task_with_task_name(
        pool: &TaskDbPool,
        task_name: TaskName,
        status: &[u8],
    ) -> Result<Option<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::get_task_with_task_name(pool.read_ref(), task_name, status).await?)
    }

    pub async fn list_tasks_with_task_name(
        pool: &TaskDbPool,
        task_name: TaskName,
        status: &[u8],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list_tasks_with_task_name(pool.read_ref(), task_name, status).await?)
    }

    pub async fn get_tasks_with_request_body(
        pool: &TaskDbPool,
        keyword: &str,
        status: &[u8],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::get_tasks_with_request_body(pool.read_ref(), keyword, status).await?)
    }

    pub async fn get_tasks_with_request_body_and_task_name(
        pool: &TaskDbPool,
        task_name: TaskName,
        keyword: &str,
        status: &[u8],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::get_tasks_with_request_body_and_task_name(
            pool.read_ref(),
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
        Ok(TaskQueueDao::update_task_remark(pool.write_ref(), id, remark).await?)
    }

    pub async fn delete_task(pool: &TaskDbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete(pool.write_ref(), id).await?)
    }

    pub async fn done_task_queue(pool: &TaskDbPool) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.read_ref(), Some(2), None).await?)
    }

    pub async fn failed_task_queue(
        pool: &TaskDbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.read_ref(), Some(3), None).await?)
    }

    pub async fn failed_mqtt_task_queue(
        pool: &TaskDbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.read_ref(), Some(3), Some(2)).await?)
    }

    pub async fn running_task_queue(
        pool: &TaskDbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.read_ref(), Some(1), None).await?)
    }

    pub async fn hanging_task_queue(
        pool: &TaskDbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.read_ref(), Some(4), None).await?)
    }

    pub async fn pending_task_queue(
        pool: &TaskDbPool,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error> {
        Ok(TaskQueueDao::list(pool.read_ref(), Some(0), None).await?)
    }

    pub async fn task_running(pool: &TaskDbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::update_status(pool.write_ref(), id, 1).await?)
    }

    pub async fn task_done(pool: &TaskDbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::update_status(pool.write_ref(), id, 2).await?)
    }

    pub async fn task_hang_up(pool: &TaskDbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::update_status(pool.write_ref(), id, 4).await?)
    }

    pub async fn increase_retry_times(pool: &TaskDbPool, id: &str) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::increase_retry_times(pool.write_ref(), id).await?)
    }

    pub async fn delete_old(pool: &TaskDbPool, day: u16) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete_old(pool.write_ref(), day).await?)
    }

    pub async fn delete_oldest_by_status_when_exceeded(
        pool: &TaskDbPool,
        max_size: u32,
        target_status: u8,
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete_oldest_by_status_when_exceeded(
            pool.write_ref(),
            max_size,
            target_status,
        )
        .await?)
    }

    pub async fn delete_all(pool: &TaskDbPool, typ: Option<u8>) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete_all(pool.write_ref(), typ).await?)
    }

    pub async fn has_unfinished_task(pool: &TaskDbPool) -> Result<bool, crate::Error> {
        Ok(TaskQueueDao::has_unfinished_task(pool.read_ref()).await?)
    }

    pub async fn delete_tasks_with_request_body_like(
        pool: &TaskDbPool,
        keyword: &str,
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::delete_tasks_with_request_body_like(pool.write_ref(), keyword).await?)
    }

    /// 批量插入TaskQueueEntity
    pub async fn insert_batch_task(
        pool: &TaskDbPool,
        tasks: &[TaskQueueEntity],
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::insert_batch(pool.write_ref(), tasks).await?)
    }

    /// 批量插入TaskQueueEntity，忽略冲突
    pub async fn insert_batch_task_ignore_conflict(
        pool: &TaskDbPool,
        tasks: &[TaskQueueEntity],
    ) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::insert_batch_ignore_conflict(pool.write_ref(), tasks).await?)
    }

    /// 获取task_queue表记录数
    pub async fn count_tasks(pool: &TaskDbPool) -> Result<i64, crate::Error> {
        Ok(TaskQueueDao::count(pool.read_ref()).await?)
    }

    /// 获取core_db中task_queue表记录数
    pub async fn count_tasks_core(pool: &CoreDbPool) -> Result<i64, crate::Error> {
        Ok(TaskQueueDao::count(pool.read_ref()).await?)
    }

    /// 冻结旧表
    pub async fn freeze_table(pool: &TaskDbPool) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::freeze_table(pool.write_ref()).await?)
    }

    /// 冻结core_db中的旧表
    pub async fn freeze_table_core(pool: &CoreDbPool) -> Result<(), crate::Error> {
        Ok(TaskQueueDao::freeze_table(pool.write_ref()).await?)
    }

    /// 检查表是否存在
    pub async fn table_exists(pool: &TaskDbPool, table_name: &str) -> Result<bool, crate::Error> {
        Ok(TaskQueueDao::table_exists(pool.read_ref(), table_name).await?)
    }

    /// 检查core_db中表是否存在
    pub async fn table_exists_core(
        pool: &CoreDbPool,
        table_name: &str,
    ) -> Result<bool, crate::Error> {
        Ok(TaskQueueDao::table_exists(pool.read_ref(), table_name).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::TaskQueueRepo;
    use crate::{
        dao::task_queue::TaskQueueDao,
        entities::task_queue::{KnownTaskName, TaskName},
    };

    fn make_temp_dir(prefix: &str) -> String {
        let dir = std::env::temp_dir().join(format!(
            "{}_{}_{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().to_string()
    }

    async fn setup_task_pool(prefix: &str) -> crate::TaskDbPool {
        let dir = make_temp_dir(prefix);
        let ctx = crate::SqliteContext::new(&dir, Some("task.db")).await.unwrap();
        ctx.into_task_db_pool().unwrap()
    }

    #[test]
    fn task_queue_repo_build_backend_task_sets_backend_type() {
        let entity = TaskQueueRepo::build_backend_task(
            TaskName::Known(KnownTaskName::PullHotCoins),
            Some("{\"k\":\"v\"}".to_string()),
            Some("remark".to_string()),
        )
        .unwrap();

        assert_eq!(entity.r#type, 1);
        assert_eq!(entity.status, 0);
        assert_eq!(entity.remark.as_deref(), Some("remark"));
        assert_eq!(entity.request_body.as_deref(), Some("{\"k\":\"v\"}"));
    }

    #[test]
    fn task_queue_repo_build_mqtt_task_sets_mqtt_type_and_fixed_id() {
        let entity = TaskQueueRepo::build_mqtt_task(
            "fixed-id",
            TaskName::Known(KnownTaskName::PullApiWalletCoins),
            Some("{\"a\":1}".to_string()),
            None,
        )
        .unwrap();

        assert_eq!(entity.id, "fixed-id");
        assert_eq!(entity.r#type, 2);
        assert_eq!(entity.status, 0);
    }

    #[tokio::test]
    async fn task_queue_repo_create_and_detail_success() {
        let pool = setup_task_pool("wallet_db_task_queue_repo_success").await;
        let task = TaskQueueRepo::build_backend_task(
            TaskName::Known(KnownTaskName::PullHotCoins),
            Some("{\"k\":\"v\"}".to_string()),
            Some("remark".to_string()),
        )
        .unwrap();
        let id = task.id.clone();

        TaskQueueRepo::create_task(&pool, task).await.unwrap();
        let found = TaskQueueRepo::task_detail(&pool, &id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, id);
    }

    #[tokio::test]
    async fn task_queue_repo_detail_missing_returns_none() {
        let pool = setup_task_pool("wallet_db_task_queue_repo_edge").await;
        let found = TaskQueueRepo::task_detail(&pool, "task_missing").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn task_queue_repo_tx_rollback_keeps_task_absent() {
        let pool = setup_task_pool("wallet_db_task_queue_repo_rollback").await;
        let task = TaskQueueRepo::build_backend_task(
            TaskName::Known(KnownTaskName::PullApiWalletCoins),
            Some("{\"a\":1}".to_string()),
            None,
        )
        .unwrap();
        let id = task.id.clone();

        let mut tx = pool.write_ref().begin().await.unwrap();
        TaskQueueDao::upsert(tx.as_mut(), task).await.unwrap();
        tx.rollback().await.unwrap();

        let found = TaskQueueRepo::task_detail(&pool, &id).await.unwrap();
        assert!(found.is_none());
    }
}
