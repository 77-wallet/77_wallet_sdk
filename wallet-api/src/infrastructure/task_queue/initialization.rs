use crate::{
    domain::{
        api_wallet::{coin::ApiCoinDomain, wallet::ApiWalletDomain},
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

// 先定义枚举
pub(crate) enum InitializationTask {
    PullAnnouncement,
    PullHotCoins,
    PullApiWalletCoins,
    // ProcessUnconfirmMsg,
    SetBlockBrowserUrl,
    SetFiat,
    RecoverQueueData,
    InitMqtt,
    RecoverAddrExpandComplete,
    CacheSeed,
}

// 然后实现Trait
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
            InitializationTask::RecoverAddrExpandComplete => {
                TaskName::Known(KnownTaskName::RecoverAddrExpandComplete)
            }
            InitializationTask::CacheSeed => TaskName::Known(KnownTaskName::CacheSeed),
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
                // 从后端获取最新的币数据
                let coins = ApiCoinDomain::pull_api_coins().await?;

                // 只有当成功获取到新数据时，才更新币价和初始化
                if !coins.is_empty() {
                    ApiCoinDomain::init_token_price().await?;

                    let list = ApiCoinRepo::coin_list(&pool).await?;

                    // 准备批量初始化币价的数据
                    let mut coins_to_initialize = Vec::with_capacity(list.len());
                    for coin in list.iter() {
                        if let Ok(price_real) = wallet_utils::unit::string_to_f64(&coin.price) {
                            coins_to_initialize.push(crate::infrastructure::asset_calc::actor_model::CoinInitializationData {
                                symbol: coin.symbol.clone(),
                                chain_code: coin.chain_code.clone(),
                                name: coin.name.clone(),
                                token_address: coin.token_address.clone(),
                                price_real,
                                decimals: coin.decimals,
                            });
                        }
                    }

                    // 批量初始化币价
                    if !coins_to_initialize.is_empty() {
                        let asset_calc_actor_manager = crate::context::CONTEXT
                            .get()
                            .unwrap()
                            .get_global_asset_calc_actor_manager()
                            .await?;
                        asset_calc_actor_manager
                            .batch_initialize_prices(coins_to_initialize)
                            .await?;
                    }

                    // 添加支持的币种
                    ApiCoinDomain::add_supported_coin(coins).await?;
                } else {
                    tracing::warn!("No new coin data received from backend API");
                }
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
            InitializationTask::RecoverAddrExpandComplete => {
                tracing::debug!("recover address expand complete start");
                crate::infrastructure::expand_address::service::ExpandService::recover_unfinished_items().await?;
                crate::infrastructure::expand_address::service::ExpandService::recover_unfinished_complete().await?;
                tracing::debug!("recover address expand complete end");
            }
            InitializationTask::CacheSeed => {
                tracing::debug!("cache seed start");
                ApiWalletDomain::set_all_wallet_seed().await?;
                tracing::debug!("cache seed end");
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
