use crate::{
    domain,
    infrastructure::{
        self, SharedCache,
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
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::{RwLock, mpsc::UnboundedSender};
use wallet_database::{
    SqliteContext, factory::RepositoryFactory, repositories::device::DeviceRepo,
};

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

pub type FrontendNotifySender = Option<tokio::sync::mpsc::UnboundedSender<FrontendNotifyEvent>>;

#[derive(Debug, Clone)]
pub struct Context {
    pub(crate) dirs: Dirs,
    pub(crate) aggregate_api: Arc<String>,
    pub(crate) backend_api: wallet_transport_backend::api::BackendApi,
    pub(crate) sqlite_context: wallet_database::SqliteContext,
    pub(crate) oss_client: wallet_oss::oss_client::OssClient,
    pub(crate) frontend_notify: Arc<RwLock<FrontendNotifySender>>,
    pub(crate) task_manager: TaskManager,
    pub(crate) mqtt_topics: Arc<RwLock<Topics>>,
    pub(crate) rpc_token: Arc<RwLock<RpcToken>>,
    pub(crate) device: Arc<DeviceInfo>,
    pub(crate) cache: Arc<SharedCache>,
    pub(crate) inner_event_handle: InnerEventHandle,
    pub(crate) unconfirmed_msg_collector: UnconfirmedMsgCollector,
    pub(crate) unconfirmed_msg_processor: UnconfirmedMsgProcessor,
    pub(crate) process_withdraw_tx_handle: Arc<ProcessWithdrawTxHandle>,
    pub(crate) process_fee_tx_handle: Arc<ProcessFeeTxHandle>,
}

pub(crate) static CONTEXT: once_cell::sync::Lazy<tokio::sync::OnceCell<Context>> =
    once_cell::sync::Lazy::new(tokio::sync::OnceCell::new);

pub(crate) async fn init_context<'a>(
    sn: &str,
    device_type: &str,
    dirs: Dirs,
    frontend_notify: Option<tokio::sync::mpsc::UnboundedSender<FrontendNotifyEvent>>,
    config: crate::Config,
) -> Result<&'a Context, crate::ServiceError> {
    let context = CONTEXT
        .get_or_try_init::<crate::ServiceError, _, _>(|| async {
            let context = Context::new(sn, device_type, dirs, frontend_notify, config).await?;
            Ok(context)
        })
        .await?;

    Ok(context)
}

impl Context {
    async fn new(
        sn: &str,
        device_type: &str,
        dirs: Dirs,
        frontend_notify: FrontendNotifySender,
        config: crate::Config,
    ) -> Result<Context, crate::ServiceError> {
        let sqlite_context = SqliteContext::new(&dirs.db_dir.to_string_lossy()).await?;

        let client_id = crate::domain::app::DeviceDomain::client_device_by_sn(sn, device_type);

        #[cfg(feature = "dev")]
        let api_url = config.backend_api.dev_url;
        #[cfg(feature = "test")]
        let api_url = config.backend_api.test_url;
        #[cfg(feature = "prod")]
        let api_url = config.backend_api.prod_url;

        // 聚合器api
        #[cfg(feature = "test")]
        let aggregate_api = config.aggregate_api.test_url;
        #[cfg(feature = "prod")]
        let aggregate_api = config.aggregate_api.prod_url;

        let headers_opt = Some(HashMap::from([("clientId".to_string(), client_id.clone())]));
        let aes_cbc_cryptor =
            wallet_utils::cbc::AesCbcCryptor::new(&config.crypto.aes_key, &config.crypto.aes_iv);
        let backend_api = wallet_transport_backend::api::BackendApi::new(
            Some(api_url.to_string()),
            headers_opt,
            aes_cbc_cryptor,
        )?;

        let frontend_notify = Arc::new(RwLock::new(frontend_notify));

        {
            let mut app_state = crate::app_state::APP_STATE.write().await;
            app_state.set_backend_url(Some(backend_api.base_url.clone()));
        }

        let oss_client = wallet_oss::oss_client::OssClient::new(&config.oss);

        let unconfirmed_msg_collector = UnconfirmedMsgCollector::new();
        // 创建 TaskManager 实例
        let notify = Arc::new(tokio::sync::Notify::new());
        let task_manager = TaskManager::new(notify.clone());

        let unconfirmed_msg_processor = UnconfirmedMsgProcessor::new(&client_id, notify);

        let inner_event_handle = InnerEventHandle::new();

        let process_withdraw_tx_handle = ProcessWithdrawTxHandle::new().await;

        let process_fee_tx_handle = ProcessFeeTxHandle::new().await;

        Ok(Context {
            dirs,
            backend_api,
            aggregate_api: Arc::new(aggregate_api),
            sqlite_context,
            frontend_notify,
            oss_client,
            task_manager,
            mqtt_topics: Arc::new(RwLock::new(Topics::new())),
            rpc_token: Arc::new(RwLock::new(RpcToken::default())),
            device: Arc::new(DeviceInfo::new(sn, &client_id)),
            cache: Arc::new(SharedCache::new()),
            inner_event_handle,
            unconfirmed_msg_collector,
            unconfirmed_msg_processor,
            process_withdraw_tx_handle: Arc::new(process_withdraw_tx_handle),
            process_fee_tx_handle: Arc::new(process_fee_tx_handle),
        })
    }

    pub async fn set_frontend_notify_sender(
        frontend_notify: FrontendNotifySender,
    ) -> Result<(), crate::ServiceError> {
        let cx = Context::get_context()?;
        let mut lock = cx.frontend_notify.write().await;
        *lock = frontend_notify;
        Ok(())
    }

    pub(crate) fn get_context() -> Result<&'static Context, crate::SystemError> {
        CONTEXT.get().ok_or(crate::SystemError::ContextNotInit)
    }

    pub(crate) fn get_global_sqlite_pool()
    -> Result<std::sync::Arc<sqlx::SqlitePool>, crate::ServiceError> {
        Ok(Context::get_context()?.sqlite_context.get_pool()?.clone())
    }

    pub(crate) fn get_global_backend_api()
    -> Result<&'static wallet_transport_backend::api::BackendApi, crate::ServiceError> {
        Ok(&Context::get_context()?.backend_api)
    }

    pub(crate) fn get_global_dirs() -> Result<&'static crate::manager::Dirs, crate::SystemError> {
        Ok(&Context::get_context()?.dirs)
    }

    pub(crate) fn get_aggregate_api() -> Result<String, crate::SystemError> {
        Ok((&Context::get_context()?.aggregate_api.clone()).to_string())
    }

    pub(crate) fn get_global_oss_client()
    -> Result<&'static wallet_oss::oss_client::OssClient, crate::SystemError> {
        Ok(&Context::get_context()?.oss_client)
    }

    pub(crate) fn get_global_task_manager() -> Result<&'static TaskManager, crate::SystemError> {
        Ok(&Context::get_context()?.task_manager)
    }

    pub(crate) fn get_global_cache() -> Result<Arc<SharedCache>, crate::SystemError> {
        Ok(Context::get_context()?.cache.clone())
    }

    pub(crate) fn get_global_mqtt_topics()
    -> Result<&'static std::sync::Arc<RwLock<Topics>>, crate::SystemError> {
        Ok(&Context::get_context()?.mqtt_topics)
    }

    pub(crate) fn get_global_frontend_notify_sender() -> Result<
        &'static std::sync::Arc<RwLock<crate::manager::FrontendNotifySender>>,
        crate::SystemError,
    > {
        Ok(&Context::get_context()?.frontend_notify)
    }

    pub(crate) async fn get_rpc_header()
    -> Result<std::collections::HashMap<String, String>, crate::ServiceError> {
        let cx = Context::get_context()?;

        let token_expired = {
            let token_guard = cx.rpc_token.read().await;
            token_guard.instance < tokio::time::Instant::now()
        };

        if token_expired {
            let backend_api = cx.backend_api.clone();
            let new_token_response = backend_api.rpc_token(&cx.device.client_id).await;
            match new_token_response {
                Ok(token) => {
                    let new_token = RpcToken {
                        token,
                        instance: tokio::time::Instant::now()
                            + tokio::time::Duration::from_secs(30 * 60),
                    };

                    {
                        let mut token_guard = cx.rpc_token.write().await;
                        *token_guard = new_token.clone();
                    }

                    Ok(HashMap::from([("token".to_string(), new_token.token)]))
                }
                Err(e) => {
                    // 服务端报错,如果token有值那么使用原来的值，服务端token的过期时间会大于我本地的。

                    let token_guard = cx.rpc_token.read().await;
                    let token = token_guard.token.clone();
                    if !token.is_empty() {
                        Ok(HashMap::from([("token".to_string(), token)]))
                    } else {
                        Err(crate::BusinessError::Chain(crate::ChainError::NodeToken(
                            e.to_string(),
                        )))?
                    }
                }
            }
        } else {
            // 未过期使用缓存里面的token
            let token_guard = cx.rpc_token.read().await;
            let token = token_guard.token.clone();

            Ok(HashMap::from([("token".to_string(), token)]))
        }
    }

    pub(crate) fn get_global_inner_event_handle()
    -> Result<&'static InnerEventHandle, crate::SystemError> {
        Ok(&Context::get_context()?.inner_event_handle)
    }

    pub(crate) fn get_global_notify() -> Result<Arc<tokio::sync::Notify>, crate::SystemError> {
        Ok(Context::get_context()?.task_manager.notify.clone())
    }

    pub(crate) fn get_global_unconfirmed_msg_collector()
    -> Result<&'static UnconfirmedMsgCollector, crate::SystemError> {
        Ok(&Context::get_context()?.unconfirmed_msg_collector)
    }

    pub(crate) fn get_global_unconfirmed_msg_processor()
    -> Result<&'static UnconfirmedMsgProcessor, crate::SystemError> {
        Ok(&Context::get_context()?.unconfirmed_msg_processor)
    }

    pub(crate) fn get_global_processed_withdraw_tx_handle()
    -> Result<Arc<ProcessWithdrawTxHandle>, crate::SystemError> {
        Ok(Context::get_context()?.process_withdraw_tx_handle.clone())
    }

    pub(crate) fn get_global_processed_fee_tx_handle()
    -> Result<Arc<ProcessFeeTxHandle>, crate::SystemError> {
        Ok(Context::get_context()?.process_fee_tx_handle.clone())
    }
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
