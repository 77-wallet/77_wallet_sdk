use crate::{
    domain,
    infrastructure::{
        self,
        inner_event::InnerEventHandle,
        process_fee_tx::ProcessFeeTxHandle,
        process_unconfirm_msg::{UnconfirmedMsgCollector, UnconfirmedMsgProcessor},
        process_withdraw_tx::ProcessWithdrawTxHandle,
        task_queue::{
            BackendApiTask, BackendApiTaskData, InitializationTask, task::Tasks,
            task_manager::TaskManager,
        },
    },
    messaging::{mqtt::subscribed::Topics, notify::FrontendNotifyEvent},
    service::node::NodeService,
};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{ mpsc::UnboundedSender};
use wallet_database::{
    factory::RepositoryFactory, repositories::device::DeviceRepo,
};
use crate::context::{init_context, Context};

/// Marks whether initialization has already been performed to prevent duplication.
/// - `OnceCell<()>` stores no real data, only acts as a flag.
/// - Combined with `Lazy` to ensure the cell itself is created only once.
pub(crate) static INIT_DATA: once_cell::sync::Lazy<tokio::sync::OnceCell<()>> =
    once_cell::sync::Lazy::new(tokio::sync::OnceCell::new);

async fn do_some_init<'a>() -> Result<&'a (), crate::ServiceError> {
    INIT_DATA.get_or_try_init(|| async { init_some_data().await }).await
}

pub async fn init_some_data() -> Result<(), crate::ServiceError> {
    crate::domain::app::config::ConfigDomain::init_url().await?;

    let pool = Context::get_global_sqlite_pool()?;
    let repo = RepositoryFactory::repo(pool.clone());
    let mut node_service = NodeService::new(repo);
    node_service.init_chain_info().await?;
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

    let sn = Context::get_context()?.device.sn.clone();
    let _ = domain::app::config::ConfigDomain::fetch_min_config(&sn).await;

    let device = DeviceRepo::get_device_info(&pool).await?;

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

pub type FrontendNotifySender = Option<tokio::sync::mpsc::UnboundedSender<FrontendNotifyEvent>>;



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

#[derive(Debug, Clone)]
pub struct WalletManager {
    pub repo_factory: RepositoryFactory,
}

impl WalletManager {
    pub async fn new(
        sn: &str,
        device_type: &str,
        sender: Option<UnboundedSender<FrontendNotifyEvent>>,
        config: crate::Config,
        dir: Dirs,
    ) -> Result<WalletManager, crate::ServiceError> {
        let base_path = infrastructure::log::LogBasePath(dir.get_log_dir());
        let context = init_context(sn, device_type, dir, sender, config).await?;
        // 以前的上报日志
        // crate::domain::log::periodic_log_report(std::time::Duration::from_secs(60 * 60)).await;

        // 现在的上报日志
        infrastructure::log::start_upload_scheduler(base_path, 5 * 60, context.oss_client.clone())
            .await?;

        Context::get_global_unconfirmed_msg_processor()?.start().await;
        Context::get_global_task_manager()?.start_task_check().await?;
        let pool = context.sqlite_context.get_pool()?;
        let repo_factory = wallet_database::factory::RepositoryFactory::new(pool);

        let manager = WalletManager { repo_factory };

        Ok(manager)
    }

    pub async fn init_data(&self) -> Result<(), crate::ServiceError> {
        // TODO ： 某个版本进行取消,
        domain::app::DeviceDomain::check_wallet_password_is_null().await?;

        tokio::spawn(async move {
            if let Err(e) = do_some_init().await {
                tracing::error!("init_data error: {}", e);
            };
        });

        Ok(())
    }

    pub async fn init_log(
        level: Option<&str>,
        app_code: &str,
        dirs: &Dirs,
        sn: &str,
    ) -> Result<(), crate::ServiceError> {
        // 修改后的版本
        let format =
            infrastructure::log::CustomEventFormat::new(app_code.to_string(), sn.to_string());

        let level = level.unwrap_or("info");

        let path = infrastructure::log::LogBasePath(dirs.get_log_dir());
        infrastructure::log::init_logger(format, path, level)?;

        Ok(())

        // 以前的版本,

        // wallet_utils::log::set_app_code(app_code);
        // let log_dir = dirs.get_log_dir();

        // wallet_utils::log::set_sn_code(sn);

        // Ok(wallet_utils::log::file::init_log(
        //     log_dir.to_string_lossy().as_ref(),
        //     level,
        // )?)
    }

    pub async fn set_frontend_notify_sender(
        &self,
        sender: UnboundedSender<FrontendNotifyEvent>,
    ) -> Result<(), crate::ServiceError> {
        Context::set_frontend_notify_sender(Some(sender)).await
    }

    pub async fn close(&self) -> Result<(), crate::ServiceError> {
        let withdraw_handle = Context::get_global_processed_withdraw_tx_handle()?;
        withdraw_handle.close().await?;
        let fee_handle = Context::get_global_processed_fee_tx_handle()?;
        fee_handle.close().await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Dirs {
    pub root_dir: PathBuf,
    pub wallet_dir: PathBuf,
    pub export_dir: PathBuf,
    pub db_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl Dirs {
    pub fn join_path(root_dir: &str, sub_path: &str) -> PathBuf {
        PathBuf::from(root_dir).join(sub_path)
    }

    pub fn new(root_dir: &str) -> Result<Dirs, crate::ServiceError> {
        let wallet_dir = Self::join_path(root_dir, "wallet_data");

        let db_dir = Self::join_path(root_dir, "db");
        let export_dir = Self::join_path(root_dir, "export");
        let log_dir = Self::join_path(root_dir, "log");

        for dir in [&db_dir, &export_dir, &log_dir, &wallet_dir] {
            wallet_utils::file_func::create_dir_all(dir)?;
        }

        Ok(Dirs { root_dir: PathBuf::from(root_dir), wallet_dir, export_dir, db_dir, log_dir })
    }

    pub fn get_wallet_dir(&self, address: Option<&str>) -> std::path::PathBuf {
        address.map_or_else(
            || PathBuf::from(&self.wallet_dir),
            |addr| PathBuf::from(&self.wallet_dir).join(addr),
        )
    }

    pub fn get_export_dir(&self) -> std::path::PathBuf {
        self.export_dir.clone()
    }

    pub fn get_log_dir(&self) -> std::path::PathBuf {
        self.log_dir.clone()
    }

    pub(crate) fn get_root_dir(
        &self,
        wallet_address: &str,
    ) -> Result<std::path::PathBuf, crate::ServiceError> {
        let root_dir = self.wallet_dir.join(wallet_address).join("root");

        wallet_utils::file_func::create_dir_all(&root_dir)?;

        Ok(root_dir)
    }

    pub(crate) fn get_subs_dir(
        &self,
        wallet_address: &str,
    ) -> Result<std::path::PathBuf, crate::ServiceError> {
        let subs_dir = self.wallet_dir.join(wallet_address).join("subs");

        wallet_utils::file_func::create_dir_all(&subs_dir)?;

        Ok(subs_dir)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        io::Write,
    };
    use tempfile::tempdir;

    use crate::Dirs;

    #[tokio::test]
    async fn test_traverse_directory_structure() -> Result<(), anyhow::Error> {
        // 创建临时目录结构
        let temp_dir = tempdir()?;
        let root_dir = temp_dir.path();

        // 创建模拟钱包和账户目录结构
        let wallet_a_path = root_dir.join("钱包A");
        let wallet_a_root_path = wallet_a_path.join("root");
        let wallet_a_subs_path = wallet_a_path.join("subs");

        let wallet_b_path = root_dir.join("钱包B");
        let wallet_b_root_path = wallet_b_path.join("root");
        let wallet_b_subs_path = wallet_b_path.join("subs");

        fs::create_dir_all(&wallet_a_root_path)?;
        fs::create_dir_all(&wallet_a_subs_path)?;
        fs::create_dir_all(&wallet_b_root_path)?;
        fs::create_dir_all(&wallet_b_subs_path)?;

        // 创建钱包根密钥文件和种子文件
        let wallet_a_root_pk_file =
            wallet_a_root_path.join("0x296a3C6B001e163409D7df318799bD52B5e3b67d-pk");
        let wallet_a_root_seed_file =
            wallet_a_root_path.join("0x296a3C6B001e163409D7df318799bD52B5e3b67d-seed");
        let wallet_b_root_pk_file =
            wallet_b_root_path.join("0x21A640a53530Aee3feEc2487a01070971d66320f-pk");
        let wallet_b_root_seed_file =
            wallet_b_root_path.join("0x21A640a53530Aee3feEc2487a01070971d66320f-seed");

        File::create(&wallet_a_root_pk_file)?.write_all(b"walletA root pk")?;
        File::create(&wallet_a_root_seed_file)?.write_all(b"walletA root seed")?;
        File::create(&wallet_b_root_pk_file)?.write_all(b"walletB root pk")?;
        File::create(&wallet_b_root_seed_file)?.write_all(b"walletB root seed")?;

        // 创建派生密钥文件
        let wallet_a_sub_key_0 = wallet_a_subs_path.join("address1-m_44'_60'_0'_0_0-pk");
        let wallet_a_sub_key_1 = wallet_a_subs_path.join("address2-m_44'_60'_0'_0_1-pk");
        let wallet_a_sub_key_2 = wallet_a_subs_path.join("address3-m_44'_60'_1'_0_0-pk");

        File::create(&wallet_a_sub_key_0)?.write_all(b"walletA sub key 0")?;
        File::create(&wallet_a_sub_key_1)?.write_all(b"walletA sub key 1")?;
        File::create(&wallet_a_sub_key_2)?.write_all(b"walletA sub key 2")?;

        let dir = &root_dir.to_string_lossy().to_string();
        let dirs = Dirs::new(dir)?;

        let config = crate::config::Config::new(&crate::test::env::get_config()?)?;
        let _manager = crate::WalletManager::new("sn", "ANDROID", None, config, dirs).await?;
        let dirs = crate::manager::Context::get_global_dirs()?;

        wallet_tree::wallet_hierarchy::v1::LegacyWalletTree::traverse_directory_structure(
            &dirs.wallet_dir,
        )?;

        Ok(())
    }
}
