use crate::infrastructure::task_queue::{
    backend::{BackendApiTask, BackendApiTaskData},
    task::Tasks,
};
use wallet_database::entities::{config::ConfigEntity, task_queue::TaskQueueEntity};
use wallet_transport_backend::request::SendMsgConfirm;

pub(crate) struct TaskQueueDomain;

impl TaskQueueDomain {
    /// 执行TaskQueue从core_db到task_db的迁移
    pub async fn migrate_task_queue_to_db() -> Result<(), crate::error::service::ServiceError> {
        let ctx = crate::context::CONTEXT.get().unwrap();

        // 1. 检查迁移状态
        let core_pool = ctx.core_pool()?;
        let migration_key = "migration.task_queue_db";
        let migration_status =
            wallet_database::dao::config::ConfigDao::find_by_key(migration_key, core_pool.as_ref())
                .await?;

        if let Some(status) = migration_status {
            if status.value == "done" {
                tracing::info!("TaskQueue migration already done, skipping");
                return Ok(());
            }
        }

        tracing::info!("Starting TaskQueue migration from core_db to task_db");

        // 2. 获取task_db连接池
        let task_pool = ctx.task_pool()?;

        // 3. 从core_db读取所有task_queue记录
        let tasks: Vec<TaskQueueEntity> =
            wallet_database::repositories::task_queue::TaskQueueRepo::all_tasks_queue(&task_pool)
                .await?;

        tracing::info!("Found {} task_queue records to migrate", tasks.len());

        // 4. 将记录插入到task_db
        wallet_database::repositories::task_queue::TaskQueueRepo::insert_batch_task(
            &task_pool, &tasks,
        )
        .await?;

        // 5. 数据校验
        let old_count: i64 =
            wallet_database::repositories::task_queue::TaskQueueRepo::count_tasks(&task_pool)
                .await?;

        let new_count: i64 =
            wallet_database::repositories::task_queue::TaskQueueRepo::count_tasks(&task_pool)
                .await?;

        if old_count != new_count {
            panic!(
                "TaskQueue migration failed: count mismatch (old: {}, new: {})\n",
                old_count, new_count
            );
        }

        tracing::info!("TaskQueue migration completed successfully, count: {}", new_count);

        // 6. 冻结旧表
        wallet_database::repositories::task_queue::TaskQueueRepo::freeze_table(&task_pool).await?;

        tracing::info!("Froze old task_queue table as task_queue_legacy");

        // 7. 更新迁移标记
        wallet_database::dao::config::ConfigDao::upsert(
            migration_key,
            "done",
            Some(0),
            core_pool.as_ref(),
        )
        .await?;

        tracing::info!("Updated migration status to done");

        Ok(())
    }

    pub async fn send_msg_confirm(
        ids: Vec<SendMsgConfirm>,
    ) -> Result<(), crate::error::service::ServiceError> {
        if !ids.is_empty() {
            const BATCH_SIZE: usize = 500;
            for chunk in ids.chunks(BATCH_SIZE) {
                let api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
                api.send_msg_confirm(&wallet_transport_backend::request::SendMsgConfirmReq::new(
                    chunk.to_vec(),
                ))
                .await?;
            }
        }
        Ok(())
    }

    // send a request to backend if failed wrap to task
    pub async fn send_or_wrap_task<T: serde::Serialize + std::fmt::Debug>(
        req: T,
        endpoint: &str,
    ) -> Result<Option<BackendApiTask>, crate::error::service::ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let res = backend.post_request::<_, serde_json::Value>(endpoint, &req).await;

        if let Err(e) = res {
            tracing::error!("request backend:{},req:{:?} error:{}", endpoint, req, e);

            let task = BackendApiTask::BackendApi(BackendApiTaskData::new(endpoint, &req)?);
            return Ok(Some(task));
        }
        Ok(None)
    }

    // 发送任务,如果失败放入到队列中去
    pub async fn send_or_to_queue<T: serde::Serialize + std::fmt::Debug>(
        req: T,
        endpoint: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let task = Self::send_or_wrap_task(req, endpoint).await?;

        if let Some(task) = task {
            Tasks::new().push(task).send().await?;
        }

        Ok(())
    }
}
