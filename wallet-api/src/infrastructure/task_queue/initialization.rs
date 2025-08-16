use crate::{
    domain::{
        app::{config::ConfigDomain, mqtt::MqttDomain},
        multisig::MultisigQueueDomain,
    },
    infrastructure::task_queue::task::{TaskTrait, task_type::TaskType},
    service::{announcement::AnnouncementService, coin::CoinService},
};
use wallet_database::{
    entities::task_queue::{KnownTaskName, TaskName},
    factory::RepositoryFactory,
};

#[async_trait::async_trait]
impl TaskTrait for InitializationTask {
    fn get_name(&self) -> TaskName {
        match self {
            InitializationTask::PullAnnouncement => {
                TaskName::Known(KnownTaskName::PullAnnouncement)
            }
            InitializationTask::PullHotCoins => TaskName::Known(KnownTaskName::PullHotCoins),
            InitializationTask::SetBlockBrowserUrl => {
                TaskName::Known(KnownTaskName::SetBlockBrowserUrl)
            }
            InitializationTask::SetFiat => TaskName::Known(KnownTaskName::SetFiat),
            InitializationTask::RecoverQueueData => {
                TaskName::Known(KnownTaskName::RecoverQueueData)
            }
            InitializationTask::InitMqtt => TaskName::Known(KnownTaskName::InitMqtt),
        }
    }
    fn get_type(&self) -> TaskType {
        TaskType::Initialization
    }
    fn get_body(&self) -> Result<Option<String>, crate::ServiceError> {
        Ok(None)
    }

    async fn execute(&self, _id: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        match self {
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
                MqttDomain::init(&mut repo).await?;
                tracing::debug!("init mqtt end");
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(crate) enum InitializationTask {
    PullAnnouncement,
    PullHotCoins,
    // ProcessUnconfirmMsg,
    SetBlockBrowserUrl,
    SetFiat,
    RecoverQueueData,
    InitMqtt,
}
