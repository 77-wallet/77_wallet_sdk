use super::{
    handle_backend_api_task, handle_common_task, handle_initialization_task, handle_mqtt_task, Task,
};
use dashmap::DashSet;
use rand::Rng as _;
use std::sync::Arc;
use tokio::sync::Semaphore;
use wallet_database::entities::task_queue::TaskQueueEntity;
use wallet_database::repositories::task_queue::TaskQueueRepoTrait;

/// 定义共享的 running_tasks 类型
type RunningTasks = Arc<DashSet<String>>;

#[derive(Debug, Clone)]
pub struct TaskManager {
    running_tasks: RunningTasks,
    task_sender: crate::manager::TaskSender,
}

impl TaskManager {
    /// 创建一个新的 TaskManager 实例
    pub fn new() -> Self {
        let running_tasks: RunningTasks = Arc::new(DashSet::new());
        let task_sender = Self::task_dispatcher(Arc::clone(&running_tasks));
        Self {
            running_tasks,
            task_sender,
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
    pub fn get_task_sender(&self) -> tokio::sync::mpsc::UnboundedSender<Vec<TaskQueueEntity>> {
        self.task_sender.clone()
    }

    /// 任务检查函数
    async fn task_check(running_tasks: RunningTasks) {
        // 在 TaskManager 的方法中启动
        tokio::spawn(async move {
            let pool = crate::manager::Context::get_global_sqlite_pool().unwrap();

            let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

            if let Err(e) = Self::check_handle(repo, &running_tasks).await {
                tracing::error!("task check error: {}", e);
            }
        });
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

        let mut tasks = Vec::new();

        // 获取当前正在运行的任务
        for task in failed_queue {
            if !running_tasks.contains(&task.id) {
                tasks.push(task);
            }
        }

        if let Err(e) = manager.get_task_sender().send(tasks) {
            tracing::error!("send task queue error: {}", e);
        }
        Ok(())
    }

    /// 任务调度器（task_dispatcher）的设计结构
    ///
    /// 整体结构图：
    ///
    /// ```text
    ///         上层模块 / 外部调用
    ///                  │
    ///                  ▼
    ///   UnboundedSender<Vec<Task>> （无限缓冲）
    ///                  │
    ///      ┌───────────┴────────────┐
    ///      │ 拆分 Vec<Task> 并发送到 ↓
    ///      ▼                        internal task queue (Sender<Task>, 有界，例如容量 100)
    ///    dispatcher task loop  ───────────────────────┐
    ///                                                │
    ///      tokio::spawn(process_task(task)) <────────┘
    /// ```
    ///
    /// ### 模块说明：
    ///
    /// - `UnboundedSender<Vec<Task>>`:
    ///   外部任务发送入口，使用 **无界通道**，确保不会阻塞外部调用。
    ///   无论任务量多大，UI 和上层模块不会卡顿或崩溃。
    ///
    /// - `dispatcher task loop`:
    ///   异步任务派发循环，监听 `UnboundedReceiver<Vec<Task>>`，
    ///   拆分任务并发送到内部有限缓冲区（`Sender<Task>`）。
    ///
    /// - `Sender<Task>` (bounded):
    ///   有限容量的内部队列（如容量 100），限制并发任务数量，
    ///   避免资源耗尽、CPU 飙高、内存暴涨等问题。
    ///
    /// - `tokio::spawn(process_task(task))`:
    ///   异步处理单个任务逻辑，执行后自动释放任务槽位，保持系统健康运行。
    fn task_dispatcher(
        running_tasks: RunningTasks,
    ) -> tokio::sync::mpsc::UnboundedSender<Vec<TaskQueueEntity>> {
        let (external_tx, mut external_rx) =
            tokio::sync::mpsc::unbounded_channel::<Vec<TaskQueueEntity>>();

        // 内部缓冲池：有界控制最大任务数（比如100）
        let (internal_tx, mut internal_rx) = tokio::sync::mpsc::channel::<TaskQueueEntity>(1000);

        // 外部接收 Vec<Task>，拆分为单个任务送入内部 Sender
        tokio::spawn(async move {
            while let Some(task_list) = external_rx.recv().await {
                for task in task_list {
                    if let Err(e) = internal_tx.send(task).await {
                        tracing::warn!("task dispatcher dropped task: {}", e);
                    }
                }
            }
        });

        // 固定后台 Worker 拉任务处理
        let internal_running = Arc::clone(&running_tasks);
        let semaphore = Arc::new(Semaphore::new(100)); // 控制最大并发数
        tokio::spawn(async move {
            while let Some(task) = internal_rx.recv().await {
                tracing::debug!("[task_process] tasks: {task:?}");
                let task_id = task.id.clone();

                if internal_running.insert(task_id.clone()) {
                    let rt_clone = Arc::clone(&internal_running);
                    let semaphore_clone = Arc::clone(&semaphore);

                    // 每次任务获取 permit
                    let permit = semaphore_clone.acquire_owned().await.unwrap();
                    tokio::spawn(async move {
                        Self::process_single_task(task, rt_clone).await;
                        drop(permit); // 执行完释放 permit
                    });
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                }
            }
        });
        external_tx
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
