pub(crate) mod dispatcher;
pub(crate) mod scheduler;
use crate::{
    domain::app::config::ConfigDomain,
    error::service::ServiceError,
    infrastructure::task_queue::task::{TaskTrait, task_type::TaskType},
};

use dashmap::DashSet;
use dispatcher::{Dispatcher, PriorityTask, TaskSender};
use rand::Rng as _;
use std::{collections::BTreeMap, sync::Arc};
use wallet_database::{
    entities::task_queue::TaskQueueEntity,
    repositories::{device::DeviceRepo, task_queue::TaskQueueRepoTrait},
};
use wallet_transport_backend::{
    consts::endpoint::SEND_MSG_CONFIRM, request::ClientTaskLogUploadReq,
};

/// 定义共享的 running_tasks 类型
type RunningTasks = Arc<DashSet<String>>;

#[derive(Debug, Clone)]
pub struct TaskManager {
    running_tasks: RunningTasks,
    // task_sender: crate::manager::TaskSender,
    pub(crate) notify: Arc<tokio::sync::Notify>,
    dispatcher: Dispatcher,
}

impl TaskManager {
    /// 创建一个新的 TaskManager 实例
    pub fn new(notify: Arc<tokio::sync::Notify>) -> Self {
        let running_tasks: RunningTasks = Arc::new(DashSet::new());
        let dispatcher = Dispatcher::new(Arc::clone(&running_tasks));
        // let task_sender = dispatcher.task_dispatcher(Arc::clone(&running_tasks));
        Self {
            running_tasks,
            // task_sender,
            notify,
            dispatcher,
        }
    }

    /// 启动任务检查循环
    pub async fn start_task_check(&self) -> Result<(), ServiceError> {
        let running_tasks = Arc::clone(&self.running_tasks);

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        repo.delete_tasks_with_request_body_like(SEND_MSG_CONFIRM).await?;

        tokio::spawn(async move {
            Self::task_check(running_tasks).await;
        });
        Ok(())
    }

    /// 获取任务发送器
    pub fn get_task_sender(&self) -> TaskSender {
        self.dispatcher.external_tx.clone()
    }

    /// 任务检查函数
    async fn task_check(running_tasks: RunningTasks) {
        // 在 TaskManager 的方法中启动
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool().unwrap();

        let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        tracing::info!("task check start");
        if let Err(e) = Self::check_handle(repo, &running_tasks).await {
            tracing::error!("task check error: {}", e);
        }
        tracing::info!("task check end");
    }

    /// 检查并发送任务的处理函数
    async fn check_handle(
        mut repo: wallet_database::repositories::ResourcesRepo,
        running_tasks: &RunningTasks,
    ) -> Result<(), ServiceError> {
        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles();
        if let Some(handles) = handles.upgrade() {
            let manager = handles.get_global_task_manager();

            repo.delete_old(15).await?;

            let mut failed_queue = repo.failed_task_queue().await?;
            let pending_queue = repo.pending_task_queue().await?;
            let hanging_queue = repo.hanging_task_queue().await?;
            let running_queue = repo.running_task_queue().await?;
            failed_queue.extend(running_queue);
            failed_queue.extend(pending_queue);
            failed_queue.extend(hanging_queue);

            let mut grouped_tasks: BTreeMap<u8, Vec<TaskQueueEntity>> = BTreeMap::new();
            tracing::info!("failed_queue: {:#?}", failed_queue);
            for task_entity in failed_queue.into_iter() {
                if !running_tasks.contains(&task_entity.id) {
                    let Ok(task) = TryInto::<Box<dyn TaskTrait>>::try_into(&task_entity) else {
                        tracing::error!(
                            "task queue entity convert to task error: {}",
                            task_entity.id
                        );
                        repo.delete_task(&task_entity.id).await?;
                        continue;
                    };

                    let priority = scheduler::assign_priority(&*task, true)?;
                    grouped_tasks.entry(priority).or_default().push(task_entity);
                }
            }

            for (priority, tasks) in grouped_tasks {
                if let Err(e) = manager.get_task_sender().send(PriorityTask { priority, tasks }) {
                    tracing::error!("send task queue error: {}", e);
                }
            }
        } else {
            tracing::error!("handles is None");
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

            match Self::handle_task(&task).await {
                Ok(()) => break, // 成功
                Err(e) => {
                    tracing::error!(?task, "[task_process] error: {}", e);
                    let is_network = match &e {
                        crate::error::service::ServiceError::Transport(transport_error) => {
                            transport_error.is_network_error()
                        }
                        _ => false,
                    };
                    if is_network {
                        // 如果是网络错误，则重试
                        tracing::warn!(
                            "[process_single_task] task {} retry {} due to network error",
                            task_id,
                            retry_count
                        );
                    } else {
                        // 否则，记录错误并增加重试次数
                        if let Err(e) = Self::increase_retry_times(&task.id, retry_count).await {
                            tracing::error!("[process_single_task] error: {}", e);
                        }
                    }

                    if let Ok(pool) =
                        crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()
                    {
                        let mut repo =
                            wallet_database::factory::RepositoryFactory::repo(pool.clone());
                        let _ = repo.task_failed(&task_id).await;
                    }

                    if retry_count >= 10 {
                        tracing::warn!(
                            "[process_single_task] task {} exceeded max retries ({}), breaking",
                            task_id,
                            retry_count
                        );
                        if let Ok(pool) =
                            crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()
                        {
                            let mut repo =
                                wallet_database::factory::RepositoryFactory::repo(pool.clone());
                            let _ = repo.task_hang_up(&task_id).await;
                            tracing::warn!("[process_single_task] task {} hang up", task_id);
                        }

                        if let Err(e) = Self::upload_task_error_info(&task, &e.to_string()).await {
                            tracing::error!(
                                "[process_single_task] upload_task_error_info error: {}",
                                e
                            );
                        };

                        break;
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
        // if running_tasks.is_empty() {
        //     let notify = crate::manager::Context::get_global_notify().unwrap();
        //     notify.notify_one();
        //     tracing::info!("notify_one");
        // }
    }

    async fn upload_task_error_info(
        task_entity: &TaskQueueEntity,
        error_info: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let Some(device) = DeviceRepo::get_device_info(pool).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };

        let client_id = crate::domain::app::DeviceDomain::client_id_by_device(&device)?;
        let app_version = ConfigDomain::get_app_version().await?;

        let req = ClientTaskLogUploadReq::new(
            &device.sn,
            &client_id,
            &app_version.app_version,
            &task_entity.id,
            &wallet_utils::serde_func::serde_to_string(&task_entity.task_name)?,
            &task_entity.r#type.to_string(),
            &task_entity.request_body,
            error_info,
        );

        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        backend_api.client_task_log_upload(req).await?;

        let task: Box<dyn TaskTrait> = task_entity.try_into()?;
        if task.get_type() == TaskType::Mqtt {
            let handles = crate::context::CONTEXT.get().unwrap().get_global_handles();
            if let Some(handles) = handles.upgrade() {
                let unconfirmed_msg_collector = handles.get_global_unconfirmed_msg_collector();
                tracing::info!("mqtt submit unconfirmed msg collector: {}", task_entity.id);
                unconfirmed_msg_collector.submit(vec![task_entity.id.to_string()])?;
            }
        }
        Ok(())
    }

    async fn increase_retry_times(
        task_id: &str,
        retry_count: i32,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        if retry_count > 0 {
            repo.increase_retry_times(task_id).await?;
        }

        Ok(())
    }

    async fn handle_task(
        task_entity: &TaskQueueEntity,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        let id = task_entity.id.clone();
        let task: Box<dyn TaskTrait> = task_entity.try_into()?;
        let task_type = task.get_type(); // update task running status

        repo.task_running(&id).await?;

        task.execute(&id).await?;

        repo.task_done(&id).await?;

        if task_type == TaskType::Mqtt {
            let handles = crate::context::CONTEXT.get().unwrap().get_global_handles();
            if let Some(handles) = handles.upgrade() {
                let unconfirmed_msg_collector = handles.get_global_unconfirmed_msg_collector();
                tracing::info!("mqtt submit unconfirmed msg collector: {}", id);
                unconfirmed_msg_collector.submit(vec![id])?;
            }
        }

        Ok(())
    }
}
