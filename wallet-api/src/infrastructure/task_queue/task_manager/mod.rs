mod dispatcher;
pub(crate) mod scheduler;
use super::{
    handle_backend_api_task, handle_common_task, handle_initialization_task, handle_mqtt_task, Task,
};
use dashmap::DashSet;
use dispatcher::{Dispatcher, TaskSender};
use rand::Rng as _;
use std::collections::BTreeMap;
use std::sync::Arc;
use wallet_database::entities::task_queue::TaskQueueEntity;
use wallet_database::repositories::task_queue::TaskQueueRepoTrait;

/// 定义共享的 running_tasks 类型
type RunningTasks = Arc<DashSet<String>>;

#[derive(Debug, Clone)]
pub struct TaskManager {
    running_tasks: RunningTasks,
    // task_sender: crate::manager::TaskSender,
    dispatcher: Dispatcher,
}

impl TaskManager {
    /// 创建一个新的 TaskManager 实例
    pub fn new() -> Self {
        let running_tasks: RunningTasks = Arc::new(DashSet::new());
        let dispatcher = Dispatcher::new(Arc::clone(&running_tasks));
        // let task_sender = dispatcher.task_dispatcher(Arc::clone(&running_tasks));
        Self {
            running_tasks,
            // task_sender,
            dispatcher,
        }
    }

    /// 启动任务检查循环
    pub fn start_task_check(&self) {
        let running_tasks = Arc::clone(&self.running_tasks);
        tokio::spawn(async move {
            Self::task_check(running_tasks).await;
        });
    }

    /// 获取任务发送器
    pub fn get_task_sender(&self) -> TaskSender {
        self.dispatcher.external_tx.clone()
    }

    /// 任务检查函数
    async fn task_check(running_tasks: RunningTasks) {
        // 在 TaskManager 的方法中启动
        let pool = crate::manager::Context::get_global_sqlite_pool().unwrap();

        let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        if let Err(e) = Self::check_handle(repo, &running_tasks).await {
            tracing::error!("task check error: {}", e);
        }
    }

    /// 检查并发送任务的处理函数
    async fn check_handle(
        mut repo: wallet_database::repositories::ResourcesRepo,
        running_tasks: &RunningTasks,
    ) -> Result<(), crate::ServiceError> {
        let manager = crate::manager::Context::get_global_task_manager()?;

        repo.delete_old(15).await?;

        let mut failed_queue = repo.failed_task_queue().await?;
        let pending_queue = repo.pending_task_queue().await?;
        let running_queue = repo.running_task_queue().await?;
        failed_queue.extend(running_queue);
        failed_queue.extend(pending_queue);

        let mut grouped_tasks: BTreeMap<u8, Vec<TaskQueueEntity>> = BTreeMap::new();

        tracing::info!("failed_queue: {:#?}", failed_queue);
        for task in failed_queue.into_iter() {
            if !running_tasks.contains(&task.id) {
                let priority = scheduler::assign_priority(&task, true)?;
                grouped_tasks.entry(priority).or_default().push(task);
            }
        }

        for (priority, tasks) in grouped_tasks {
            if let Err(e) = manager.get_task_sender().send((priority, tasks)) {
                tracing::error!("send task queue error: {}", e);
            }
        }
        Ok(())
    }

    async fn process_single_task(task: TaskQueueEntity, running_tasks: RunningTasks) {
        let task_id = task.id.clone();

        let mut retry_count = 0;
        let mut delay = 200; // 初始延迟设为 200 毫秒
                             // const MAX_RETRY_COUNT: i32 = 5;

        loop {
            // if retry_count >= MAX_RETRY_COUNT {
            //     tracing::warn!(
            //         "[process_single_task] task {} exceeded max retries ({})",
            //         task_id,
            //         MAX_RETRY_COUNT
            //     );
            //     if let Ok(pool) = crate::manager::Context::get_global_sqlite_pool() {
            //         let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
            //         let _ = repo.task_failed(&task_id).await;
            //     };
            //     break;
            // }

            match Self::handle_task(&task, retry_count).await {
                Ok(()) => break, // 成功
                Err(e) => {
                    tracing::error!(?task, "[task_process] error: {}", e);
                    if let Ok(pool) = crate::manager::Context::get_global_sqlite_pool() {
                        let mut repo =
                            wallet_database::factory::RepositoryFactory::repo(pool.clone());
                        let _ = repo.task_failed(&task_id).await;
                    }
                }
            }

            // 计算指数退避的延迟时间，单位是毫秒
            delay = std::cmp::min(delay * 2, 120_000); // 最大延迟设为 120 秒（120,000 毫秒）
            let jitter =
                std::time::Duration::from_millis(rand::thread_rng().gen_range(0..(delay / 2)));
            delay += jitter.as_millis() as u64; // 将延迟加上抖动
            retry_count += 1;

            tracing::debug!(
                "[process_single_task] delay: {delay} ms, retry_count: {retry_count}, jitter: {jitter:?}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }

        running_tasks.remove(&task_id);
    }

    async fn handle_task(
        task_entity: &TaskQueueEntity,
        retry_count: i32,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        let id = task_entity.id.clone();
        let task: Task = task_entity.try_into()?;
        let backend_api = crate::manager::Context::get_global_backend_api()?;
        let aes_cbc_cryptor = crate::manager::Context::get_global_aes_cbc_cryptor()?;
        // update task running status
        if retry_count > 0 {
            repo.increase_retry_times(&id).await?;
        }
        repo.task_running(&id).await?;

        match task {
            Task::Initialization(initialization_task) => {
                handle_initialization_task(initialization_task, pool).await
            }
            Task::BackendApi(backend_api_task) => {
                handle_backend_api_task(backend_api_task, backend_api, aes_cbc_cryptor).await
            }
            Task::Mqtt(mqtt_task) => handle_mqtt_task(mqtt_task, &id).await,
            Task::Common(common_task) => handle_common_task(common_task, pool).await,
        }?;
        repo.task_done(&id).await?;

        Ok(())
    }
}
