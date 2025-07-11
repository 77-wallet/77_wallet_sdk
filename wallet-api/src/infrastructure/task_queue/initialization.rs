use crate::domain::{
    announcement::AnnouncementDomain,
    app::{config::ConfigDomain, mqtt::MqttDomain},
    coin::CoinDomain,
    multisig::MultisigQueueDomain,
};
use wallet_database::{
    entities::task_queue::{KnownTaskName, TaskName},
    factory::RepositoryFactory,
    DbPool,
};

pub(crate) enum InitializationTask {
    PullAnnouncement,
    PullHotCoins,
    SetBlockBrowserUrl,
    SetFiat,
    RecoverQueueData,
    InitMqtt,
}
impl InitializationTask {
    pub(crate) fn get_name(&self) -> TaskName {
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

    pub(crate) fn get_body(&self) -> Result<Option<String>, crate::ServiceError> {
        Ok(None)
    }
}

// TODO： 不需要walletType
pub(crate) async fn handle_initialization_task(
    task: InitializationTask,
    pool: DbPool,
) -> Result<(), crate::ServiceError> {
    match task {
        InitializationTask::PullAnnouncement => {
            let mut repo = RepositoryFactory::repo(pool.clone());
            AnnouncementDomain::pull_announcement(&mut repo).await?;
        }
        InitializationTask::PullHotCoins => {
            let mut repo = RepositoryFactory::repo(pool.clone());
            CoinDomain::pull_hot_coins(&mut repo).await?;
            CoinDomain::init_token_price(&mut repo).await?;
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
