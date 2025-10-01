use wallet_database::{factory::RepositoryFactory, repositories::device::DeviceRepo};

use crate::{
    context::CONTEXT,
    domain::{self, chain::ChainDomain},
    infrastructure::task_queue::{
        backend::{BackendApiTask, BackendApiTaskData},
        initialization::InitializationTask,
        task::Tasks,
    },
    service::node::NodeService,
};

/// Marks whether initialization has already been performed to prevent duplication.
/// - `OnceCell<()>` stores no real data, only acts as a flag.
/// - Combined with `Lazy` to ensure the cell itself is created only once.
pub(crate) static INIT_DATA: once_cell::sync::Lazy<tokio::sync::OnceCell<()>> =
    once_cell::sync::Lazy::new(tokio::sync::OnceCell::new);

pub async fn do_some_init<'a>() -> Result<&'a (), crate::error::service::ServiceError> {
    INIT_DATA.get_or_try_init(|| async { init_some_data().await }).await
}

async fn init_some_data() -> Result<(), crate::error::service::ServiceError> {
    crate::domain::app::config::ConfigDomain::init_url().await?;

    let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
    ChainDomain::init_chain_info().await?;
    let repo = RepositoryFactory::repo(pool.clone());
    let mut node_service = NodeService::new(repo);
    node_service.init_node_info().await?;

    let mut repo = RepositoryFactory::repo(pool.clone());
    crate::domain::coin::CoinDomain::init_coins(&mut repo).await?;

    let token_query_rates_req = BackendApiTaskData::new(
        wallet_transport_backend::consts::endpoint::TOKEN_QUERY_RATES,
        &(),
    )?;

    let set_official_website_req = BackendApiTaskData::new(
        wallet_transport_backend::consts::endpoint::SYS_CONFIG_FIND_CONFIG_BY_KEY,
        &wallet_transport_backend::request::FindConfigByKey::new("OFFICIAL:WEBSITE"),
    )?;

    let set_app_install_download_req = BackendApiTaskData::new(
        wallet_transport_backend::consts::endpoint::APP_INSTALL_DOWNLOAD,
        &(),
    )?;

    // let mqtt_init_req =
    //     BackendApiTaskData::new(wallet_transport_backend::consts::endpoint::MQTT_INIT, &())?;

    let sn = CONTEXT.get().unwrap().get_global_device().sn.clone();
    let _ = domain::app::config::ConfigDomain::fetch_min_config(&sn).await;

    let device = DeviceRepo::get_device_info(pool).await?;

    let mut tasks = Tasks::new().push(InitializationTask::InitMqtt);
    if let Some(device) = device
        && device.language_init == 1
    {
        tasks = tasks
            .push(domain::app::DeviceDomain::language_init(&device, "CHINESE_SIMPLIFIED").await?);
    } else {
        tasks = tasks.push(InitializationTask::PullAnnouncement);
    }
    tasks
        .push(InitializationTask::PullHotCoins)
        .push(InitializationTask::SetBlockBrowserUrl)
        .push(InitializationTask::SetFiat)
        .push(InitializationTask::RecoverQueueData)
        .push(BackendApiTask::BackendApi(token_query_rates_req))
        .push(BackendApiTask::BackendApi(set_official_website_req))
        .push(BackendApiTask::BackendApi(set_app_install_download_req))
        .send()
        .await?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub(crate) sn: String,
    pub(crate) client_id: String,
}
impl DeviceInfo {
    pub fn new(sn: &str, client_id: &str) -> Self {
        Self { sn: sn.to_owned(), client_id: client_id.to_owned() }
    }
}

#[derive(Debug, Clone)]
pub struct RpcToken {
    pub token: String,
    pub instance: tokio::time::Instant,
}

impl Default for RpcToken {
    fn default() -> Self {
        Self { token: String::new(), instance: tokio::time::Instant::now() }
    }
}
