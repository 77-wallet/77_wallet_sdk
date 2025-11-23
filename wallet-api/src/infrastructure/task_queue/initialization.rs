use crate::{
    domain::{
        api_wallet::coin::ApiCoinDomain,
        app::{config::ConfigDomain, mqtt::MqttDomain},
        multisig::MultisigQueueDomain,
    },
    infrastructure::task_queue::task::{TaskTrait, task_type::TaskType},
    service::{announcement::AnnouncementService, coin::CoinService},
};
use wallet_database::{
    entities::task_queue::{KnownTaskName, TaskName},
    factory::RepositoryFactory,
    repositories::api_wallet::coin::ApiCoinRepo,
};

#[async_trait::async_trait]
impl TaskTrait for InitializationTask {
    fn get_name(&self) -> TaskName {
        match self {
            InitializationTask::PullAnnouncement => {
                TaskName::Known(KnownTaskName::PullAnnouncement)
            }
            InitializationTask::PullHotCoins => TaskName::Known(KnownTaskName::PullHotCoins),
            InitializationTask::PullApiWalletCoins => {
                TaskName::Known(KnownTaskName::PullApiWalletCoins)
            }
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
    fn get_body(&self) -> Result<Option<String>, crate::error::service::ServiceError> {
        Ok(None)
    }

    async fn execute(&self, _id: &str) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
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
            InitializationTask::PullApiWalletCoins => {
                let coins = ApiCoinDomain::pull_api_coins().await?;
                ApiCoinDomain::init_token_price().await?;

                let list = ApiCoinRepo::coin_list(&pool).await?;

                for coin in list.iter() {
                    let asset_calc_actor_manager = crate::context::CONTEXT
                        .get()
                        .unwrap()
                        .get_global_asset_calc_actor_manager()
                        .await?;
                    asset_calc_actor_manager
                        .update_price(
                            &coin.symbol,
                            &coin.chain_code,
                            coin.token_address.to_owned(),
                            wallet_utils::unit::string_to_f64(&coin.price)?,
                        )
                        .await?;
                }
                ApiCoinDomain::add_supported_coin(coins).await?;
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
                tracing::debug!("init mqtt start");
                MqttDomain::init_mqtt().await?;
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
    PullApiWalletCoins,
    // ProcessUnconfirmMsg,
    SetBlockBrowserUrl,
    SetFiat,
    RecoverQueueData,
    InitMqtt,
}
