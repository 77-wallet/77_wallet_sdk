use crate::{
    domain::{
        app::{config::ConfigDomain, mqtt::MqttDomain},
        multisig::MultisigQueueDomain,
    },
    service::{announcement::AnnouncementService, coin::CoinService},
};
use wallet_database::{
    entities::task_queue::{KnownTaskName, TaskName},
    factory::RepositoryFactory,
    DbPool,
};

pub(crate) enum InitializationTask {
    PullAnnouncement,
    PullHotCoins,
    InitTokenPrice,
    // ProcessUnconfirmMsg,
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
            InitializationTask::InitTokenPrice => TaskName::Known(KnownTaskName::InitTokenPrice),
            // InitializationTask::ProcessUnconfirmMsg => TaskName::ProcessUnconfirmMsg,
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

pub(crate) async fn handle_initialization_task(
    task: InitializationTask,
    pool: DbPool,
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
        // InitializationTask::ProcessUnconfirmMsg => {
        //     let repo = RepositoryFactory::repo(pool.clone());
        //     let device_service = DeviceService::new(repo);
        //     let Some(device) = device_service.get_device_info().await? else {
        //         return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        //     };

        //     if device.is_init != 1 {
        //         return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        //     }

        //     let client_id = crate::domain::app::DeviceDomain::client_id_by_device(&device)?;
        //     tokio::spawn(async move {
        //         let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        //         loop {
        //             interval.tick().await;
        //             match TaskQueueEntity::has_unfinished_task(&*pool).await {
        //                 Ok(true) => {
        //                     tracing::debug!("存在未完成任务，跳过处理未确认消息");
        //                     continue;
        //                 }
        //                 Ok(false) => {
        //                     tracing::debug!("不存在未完成任务，处理未确认消息");
        //                 }
        //                 Err(e) => {
        //                     tracing::error!("has_unfinished_task error: {}", e);
        //                     continue;
        //                 }
        //             }

        //             if let Err(e) = MqttDomain::process_unconfirm_msg(&client_id).await {
        //                 if let Err(e) = FrontendNotifyEvent::send_error(
        //                     "InitializationTask::ProcessUnconfirmMsg",
        //                     e.to_string(),
        //                 )
        //                 .await
        //                 {
        //                     tracing::error!("send_error error: {}", e);
        //                 }
        //                 tracing::error!("process unconfirm msg error:{}", e);
        //             };
        //             // tracing::warn!("处理未确认消息");
        //         }
        //     });
        // }
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
