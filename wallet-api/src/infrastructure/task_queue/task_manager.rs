use super::{
    task_handle::backend_handle::BackendTaskHandle, BackendApiTask, CommonTask, InitializationTask,
    MqttTask, Task,
};
use crate::{
    domain::{
        self,
        app::{config::ConfigDomain, mqtt::MqttDomain},
        multisig::MultisigQueueDomain,
        node::NodeDomain,
    },
    service::{announcement::AnnouncementService, coin::CoinService, device::DeviceService},
};
use dashmap::DashSet;
use rand::Rng as _;
use std::sync::Arc;
use tokio_stream::StreamExt as _;
use wallet_database::repositories::{chain::ChainRepoTrait, task_queue::TaskQueueRepoTrait};
use wallet_database::{entities::task_queue::TaskQueueEntity, factory::RepositoryFactory};

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
        let task_sender = Self::task_process(Arc::clone(&running_tasks));
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
            // let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            // let mut first = true;
            // loop {
            // interval.tick().await;
            let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

            if let Err(e) = Self::check_handle(repo, &running_tasks).await {
                tracing::error!("task check error: {}", e);
                // continue;
                // }
            }
        });
    }

    /// 检查并发送任务的处理函数
    async fn check_handle(
        mut repo: wallet_database::repositories::ResourcesRepo,
        // first: &mut bool,
        running_tasks: &RunningTasks,
    ) -> Result<(), crate::ServiceError> {
        let manager = crate::manager::Context::get_global_task_manager()?;

        repo.delete_old(30).await?;
        let mut failed_queue = repo.failed_task_queue().await?;
        let pending_queue = repo.pending_task_queue().await?;
        // if *first {
        let running_queue = repo.running_task_queue().await?;
        failed_queue.extend(running_queue);
        // *first = false;
        // }
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

    /// 定义 task_process 方法，接受共享的 running_tasks
    fn task_process(
        running_tasks: RunningTasks,
    ) -> tokio::sync::mpsc::UnboundedSender<Vec<TaskQueueEntity>> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Vec<TaskQueueEntity>>();
        let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

        tokio::spawn(async move {
            while let Some(tasks) = rx.next().await {
                tracing::debug!("[task_process] tasks: {tasks:?}");
                for task in tasks {
                    let task_id = task.id.clone();

                    // 检查并更新 running_tasks
                    if running_tasks.insert(task_id.clone()) {
                        let running_tasks_clone = Arc::clone(&running_tasks);
                        tokio::spawn(Self::process_single_task(task, running_tasks_clone));
                    }
                }
            }
        });
        tx
    }

    async fn process_single_task(task: TaskQueueEntity, running_tasks: RunningTasks) {
        let task_id = task.id.clone();

        let mut retry_count = 0;
        let mut delay = 200; // 初始延迟设为 200 毫秒

        // while retry_count <= 10 {
        loop {
            if let Err(e) = Self::handle_task(&task, retry_count).await {
                tracing::error!(?task, "[task_process] error: {}", e);
                if let Ok(pool) = crate::manager::Context::get_global_sqlite_pool() {
                    let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
                    let _ = repo.task_failed(&task_id).await;
                };
                // 计算指数退避的延迟时间，单位是毫秒
                delay = std::cmp::min(delay * 2, 120_000); // 最大延迟设为 120 秒（120,000 毫秒）
                let jitter =
                    std::time::Duration::from_millis(rand::thread_rng().gen_range(0..(delay / 2)));
                delay = delay + jitter.as_millis() as u64; // 将延迟加上抖动
                retry_count += 1;
                tracing::debug!("[process_single_task] delay: {delay} ms, retry_count: {retry_count}, jitter: {jitter:?}");
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                continue;
            }
            // 成功处理任务
            break;
        }

        // if retry_count >= 10 {
        //     tracing::error!("Task {} failed after max retries", task_id);
        // }

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
                handle_backend_api_task(backend_api_task, backend_api).await
            }
            Task::Mqtt(mqtt_task) => handle_mqtt_task(mqtt_task, &id).await,
            Task::Common(common_task) => handle_common_task(common_task, pool).await,
        }?;
        repo.task_done(&id).await?;

        Ok(())
    }
}

async fn handle_initialization_task(
    task: InitializationTask,
    pool: Arc<sqlx::Pool<sqlx::Sqlite>>,
) -> Result<(), crate::ServiceError> {
    match task {
        InitializationTask::PullAnnouncement => {
            let repo = RepositoryFactory::repo(pool.clone());
            let announcement_service = AnnouncementService::new(repo);
            let res = announcement_service.pull_announcement().await;

            res?;
        }
        InitializationTask::PullHotCoins => {
            let repo = RepositoryFactory::repo(pool.clone());
            let mut coin_service = CoinService::new(repo);
            coin_service.pull_hot_coins().await?;
            let repo = RepositoryFactory::repo(pool.clone());
            let coin_service = CoinService::new(repo);
            coin_service.init_token_price().await?;
        }
        InitializationTask::InitTokenPrice => {
            let repo = RepositoryFactory::repo(pool.clone());
            let coin_service = CoinService::new(repo);

            coin_service.init_token_price().await?;
        }
        InitializationTask::ProcessUnconfirmMsg => {
            let repo = RepositoryFactory::repo(pool.clone());
            let device_service = DeviceService::new(repo);
            let Some(device) = device_service.get_device_info().await? else {
                tracing::error!("get device info failed");
                return Ok(());
            };
            let client_id = crate::domain::app::DeviceDomain::client_id_by_device(&device)?;
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;

                    if let Err(e) = MqttDomain::process_unconfirm_msg(&client_id).await {
                        tracing::error!("process unconfirm msg error:{}", e);
                    };
                    tracing::warn!("处理未确认消息");
                }
            });
        }
        InitializationTask::SetBlockBrowserUrl => {
            let repo = RepositoryFactory::repo(pool.clone());
            let mut app_service = crate::service::app::AppService::new(repo);
            app_service.set_block_browser_url().await?;
        }
        InitializationTask::SetFiat => {
            ConfigDomain::init_currency().await?;
        }
        InitializationTask::RecoverQueueData => {
            MultisigQueueDomain::recover_all_uid_queue_data().await?;
        }
        InitializationTask::InitMqtt => {
            let mut repo = RepositoryFactory::repo(pool.clone());
            tracing::debug!("init mqtt start");
            domain::app::mqtt::MqttDomain::init(&mut repo).await?;
            tracing::debug!("init mqtt end");
        }
    }
    Ok(())
}

async fn handle_backend_api_task(
    task: BackendApiTask,
    backend_api: &wallet_transport_backend::api::BackendApi,
) -> Result<(), crate::ServiceError> {
    match task {
        BackendApiTask::BackendApi(data) => {
            BackendTaskHandle::do_handle(&data.endpoint, data.body, backend_api).await?;
        }
    }
    Ok(())
}

async fn handle_mqtt_task(task: Box<MqttTask>, id: &str) -> Result<(), crate::ServiceError> {
    match *task {
        MqttTask::OrderMultiSignAccept(data) => data.exec(&id).await?,
        MqttTask::OrderMultiSignAcceptCompleteMsg(data) => data.exec(&id).await?,
        MqttTask::OrderMultiSignServiceComplete(data) => data.exec(&id).await?,
        MqttTask::OrderMultiSignCreated(data) => data.exec(&id).await?,
        MqttTask::OrderMultiSignCancel(data) => data.exec(&id).await?,
        MqttTask::MultiSignTransAccept(data) => data.exec(&id).await?,
        MqttTask::MultiSignTransCancel(data) => data.exec(&id).await?,
        MqttTask::MultiSignTransAcceptCompleteMsg(data) => data.exec(&id).await?,
        MqttTask::AcctChange(data) => data.exec(&id).await?,
        MqttTask::Init(data) => data.exec(&id).await?,
        MqttTask::BulletinMsg(data) => data.exec(&id).await?,
        // MqttTask::RpcChange(data) => data.exec(&id).await?,
    }
    Ok(())
}

async fn handle_common_task(
    task: CommonTask,
    pool: Arc<sqlx::Pool<sqlx::Sqlite>>,
) -> Result<(), crate::ServiceError> {
    match task {
        CommonTask::QueryCoinPrice(data) => {
            let repo = RepositoryFactory::repo(pool.clone());
            let coin_service = CoinService::new(repo);
            coin_service.query_token_price(data).await?;
        }
        CommonTask::QueryQueueResult(data) => {
            domain::multisig::MultisigQueueDomain::sync_queue_status(&data.id).await?
        }
        CommonTask::RecoverMultisigAccountData(uid) => {
            domain::multisig::MultisigDomain::recover_uid_multisig_data(&uid, None).await?;
            MultisigQueueDomain::recover_all_queue_data(&uid).await?;
        }
        CommonTask::SyncNodesAndLinkToChains(data) => {
            let mut repo = RepositoryFactory::repo(pool.clone());
            let chain_codes = ChainRepoTrait::get_chain_list_all_status(&mut repo)
                .await?
                .into_iter()
                .map(|chain| chain.chain_code)
                .collect();
            NodeDomain::sync_nodes_and_link_to_chains(&mut repo, chain_codes, &data).await?;
        }
    }
    Ok(())
}
