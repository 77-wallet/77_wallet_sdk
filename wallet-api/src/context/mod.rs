pub(crate) mod api_wallet_backend;

use crate::{
    config::ChainNetwork,
    data::{DeviceInfo, RpcToken},
    dirs::Dirs,
    error::system::SystemError,
    handles::Handles,
    infrastructure::{
        // asset_calc::actor_model::AssetCalcActorManager,
        cache::SharedCache,
        expand_address::event::ExpandEventSender,
        recovery::pool::BackgroundTaskPool,
    },
    messaging::{mqtt::subscribed::Topics, notify::FrontendNotifyEvent},
};
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};
use tokio::sync::{Mutex, RwLock};
use wallet_database::{SqliteContext, entities::api_wallet::ApiWalletType};

use crate::context::api_wallet_backend::{ApiWalletBackend, RealApiWalletBackend};

pub type FrontendNotifySender = Option<tokio::sync::mpsc::UnboundedSender<FrontendNotifyEvent>>;

pub(crate) static CONTEXT: once_cell::sync::Lazy<tokio::sync::OnceCell<Context>> =
    once_cell::sync::Lazy::new(tokio::sync::OnceCell::new);

/// 安全获取上下文，如果上下文未初始化则返回错误
pub fn get_context() -> Result<&'static Context, crate::error::service::ServiceError> {
    CONTEXT.get().ok_or_else(|| crate::error::system::SystemError::ContextNotInit.into())
}

pub(crate) async fn init_context<'a>(
    sn: &str,
    device_type: &str,
    dirs: Dirs,
    frontend_notify: Option<tokio::sync::mpsc::UnboundedSender<FrontendNotifyEvent>>,
    config: crate::config::Config,
) -> Result<&'a Context, crate::error::service::ServiceError> {
    let context = CONTEXT
        .get_or_try_init::<crate::error::service::ServiceError, _, _>(|| async {
            let context = Context::new(sn, device_type, dirs, frontend_notify, config).await?;
            Ok(context)
        })
        .await?;

    Ok(context)
}

#[cfg(any(test, feature = "integration-tests"))]
pub(crate) async fn init_context_with_api_wallet_backend<'a>(
    sn: &str,
    device_type: &str,
    dirs: Dirs,
    frontend_notify: Option<tokio::sync::mpsc::UnboundedSender<FrontendNotifyEvent>>,
    config: crate::config::Config,
    api_wallet_backend: Arc<dyn ApiWalletBackend>,
) -> Result<&'a Context, crate::error::service::ServiceError> {
    let context = CONTEXT
        .get_or_try_init::<crate::error::service::ServiceError, _, _>(|| async {
            let context = Context::new_with_api_wallet_backend(
                sn,
                device_type,
                dirs,
                frontend_notify,
                config,
                api_wallet_backend,
            )
            .await?;
            Ok(context)
        })
        .await?;

    Ok(context)
}

pub struct Context {
    sn: String,
    client_id: String,
    dirs: Arc<Dirs>,
    aggregate_api: String,
    chain_network: ChainNetwork,
    backend_api: Arc<wallet_transport_backend::api::BackendApi>,
    api_wallet_backend: Arc<dyn ApiWalletBackend>,
    core_db: Arc<wallet_database::SqliteContext>, // data.db
    api_wallet_db: Arc<wallet_database::SqliteContext>, // api_wallet.db
    api_transaction_db: Arc<wallet_database::SqliteContext>, // api_transaction.db
    task_db: Arc<wallet_database::SqliteContext>, // task.db
    oss_client: Arc<wallet_oss::oss_client::OssClient>,
    frontend_notify: Arc<RwLock<FrontendNotifySender>>,
    mqtt_topics: Arc<RwLock<Topics>>,
    rpc_token: Arc<RwLock<RpcToken>>,
    rpc_token_refresh_lock: Mutex<()>,
    device: Arc<DeviceInfo>,
    cache: Arc<SharedCache>,
    current_wallet_type: Arc<RwLock<Option<ApiWalletType>>>,
    handles: RwLock<Weak<Handles>>,
    init_api_swap: Mutex<bool>,
    expand_event_tx: Arc<RwLock<Option<ExpandEventSender>>>,
    background_task_pool: Arc<BackgroundTaskPool>,
}

impl Context {
    async fn new(
        sn: &str,
        device_type: &str,
        dirs: Dirs,
        frontend_notify: FrontendNotifySender,
        config: crate::config::Config,
    ) -> Result<Context, crate::error::service::ServiceError> {
        let db_path = &dirs.db_dir.to_string_lossy();
        let core_db = SqliteContext::new(db_path, Some("data.db")).await?;
        let api_wallet_db = SqliteContext::new(db_path, Some("api_wallet.db")).await?;
        let api_transaction_db = SqliteContext::new(db_path, Some("api_transaction.db")).await?;
        let task_db = SqliteContext::new(db_path, Some("task.db")).await?;

        let client_id = crate::domain::app::DeviceDomain::client_device_by_sn(sn, device_type);
        tracing::info!(" ======================================  client id: {}", client_id);

        let chain_network = crate::config::Config::feature_chain_network();

        #[cfg(feature = "dev")]
        let api_url = config.backend_api.dev_url.clone();
        #[cfg(feature = "test")]
        let api_url = config.backend_api.test_url.clone();
        #[cfg(feature = "prod")]
        let api_url = config.backend_api.prod_url.clone();

        #[cfg(feature = "dev")]
        let aggregate_api = config.aggregate_api.dev_url.clone();
        #[cfg(feature = "test")]
        let aggregate_api = config.aggregate_api.test_url.clone();
        #[cfg(feature = "prod")]
        let aggregate_api = config.aggregate_api.prod_url.clone();

        tracing::info!("api_url: {}, client_id: {}", api_url, client_id);
        tracing::info!(
            "feature_profile: {}, network_source=backend_node, compatibility_feature_network={}, db_dir: {}",
            crate::config::Config::active_feature_profile(),
            chain_network.as_str(),
            dirs.db_dir.display()
        );
        let mut headers_opt = HashMap::new();
        headers_opt.insert("clientId".to_string(), client_id.clone());
        headers_opt.insert("AW-SEC-ID".to_string(), sn.to_string());
        let aes_cbc_cryptor =
            wallet_utils::cbc::AesCbcCryptor::new(&config.crypto.aes_key, &config.crypto.aes_iv);
        let backend_api = Arc::new(wallet_transport_backend::api::BackendApi::new(
            Some(api_url.to_string()),
            Some(headers_opt),
            aes_cbc_cryptor,
        )?);
        let api_wallet_backend = Arc::new(RealApiWalletBackend::new(backend_api.clone()));

        Self::build_context(
            sn,
            device_type,
            dirs,
            frontend_notify,
            config,
            backend_api,
            api_wallet_backend,
            aggregate_api,
            chain_network,
            client_id.clone(),
            core_db,
            api_wallet_db,
            api_transaction_db,
            task_db,
        )
        .await
    }

    async fn new_with_api_wallet_backend(
        sn: &str,
        device_type: &str,
        dirs: Dirs,
        frontend_notify: FrontendNotifySender,
        config: crate::config::Config,
        api_wallet_backend: Arc<dyn ApiWalletBackend>,
    ) -> Result<Context, crate::error::service::ServiceError> {
        let db_path = &dirs.db_dir.to_string_lossy();
        let core_db = SqliteContext::new(db_path, Some("data.db")).await?;
        let api_wallet_db = SqliteContext::new(db_path, Some("api_wallet.db")).await?;
        let api_transaction_db = SqliteContext::new(db_path, Some("api_transaction.db")).await?;
        let task_db = SqliteContext::new(db_path, Some("task.db")).await?;

        let client_id = crate::domain::app::DeviceDomain::client_device_by_sn(sn, device_type);
        tracing::info!(" ======================================  client id: {}", client_id);

        let chain_network = crate::config::Config::feature_chain_network();

        #[cfg(feature = "dev")]
        let api_url = config.backend_api.dev_url.clone();
        #[cfg(feature = "test")]
        let api_url = config.backend_api.test_url.clone();
        #[cfg(feature = "prod")]
        let api_url = config.backend_api.prod_url.clone();

        #[cfg(feature = "dev")]
        let aggregate_api = config.aggregate_api.dev_url.clone();
        #[cfg(feature = "test")]
        let aggregate_api = config.aggregate_api.test_url.clone();
        #[cfg(feature = "prod")]
        let aggregate_api = config.aggregate_api.prod_url.clone();

        tracing::info!("api_url: {}, client_id: {}", api_url, client_id);
        tracing::info!(
            "feature_profile: {}, network_source=backend_node, compatibility_feature_network={}, db_dir: {}",
            crate::config::Config::active_feature_profile(),
            chain_network.as_str(),
            dirs.db_dir.display()
        );
        let mut headers_opt = HashMap::new();
        headers_opt.insert("clientId".to_string(), client_id.clone());
        headers_opt.insert("AW-SEC-ID".to_string(), sn.to_string());
        let aes_cbc_cryptor =
            wallet_utils::cbc::AesCbcCryptor::new(&config.crypto.aes_key, &config.crypto.aes_iv);
        let backend_api = Arc::new(wallet_transport_backend::api::BackendApi::new(
            Some(api_url.to_string()),
            Some(headers_opt),
            aes_cbc_cryptor,
        )?);

        Self::build_context(
            sn,
            device_type,
            dirs,
            frontend_notify,
            config,
            backend_api,
            api_wallet_backend,
            aggregate_api,
            chain_network,
            client_id.clone(),
            core_db,
            api_wallet_db,
            api_transaction_db,
            task_db,
        )
        .await
    }

    async fn build_context(
        sn: &str,
        device_type: &str,
        dirs: Dirs,
        frontend_notify: FrontendNotifySender,
        config: crate::config::Config,
        backend_api: Arc<wallet_transport_backend::api::BackendApi>,
        api_wallet_backend: Arc<dyn ApiWalletBackend>,
        aggregate_api: String,
        chain_network: ChainNetwork,
        client_id: String,
        core_db: SqliteContext,
        api_wallet_db: SqliteContext,
        api_transaction_db: SqliteContext,
        task_db: SqliteContext,
    ) -> Result<Context, crate::error::service::ServiceError> {
        let frontend_notify = Arc::new(RwLock::new(frontend_notify));

        {
            let mut app_state = crate::app_state::APP_STATE.write().await;
            app_state.set_backend_url(Some(backend_api.base_url.clone()));
        }

        let oss_client = wallet_oss::oss_client::OssClient::new(&config.oss);

        let defaults = crate::config::runtime_defaults::recovery();
        let background_task_pool =
            Arc::new(BackgroundTaskPool::new(defaults.background_task_pool_max_concurrent));

        Ok(Context {
            sn: sn.to_string(),
            client_id: client_id.clone(),
            dirs: Arc::new(dirs),
            backend_api,
            api_wallet_backend,
            aggregate_api,
            chain_network,
            core_db: Arc::new(core_db),
            api_wallet_db: Arc::new(api_wallet_db),
            api_transaction_db: Arc::new(api_transaction_db),
            task_db: Arc::new(task_db),
            frontend_notify,
            oss_client: Arc::new(oss_client),
            mqtt_topics: Arc::new(RwLock::new(Topics::new())),
            rpc_token: Arc::new(RwLock::new(RpcToken::default())),
            rpc_token_refresh_lock: Mutex::new(()),
            device: Arc::new(DeviceInfo::new(sn, &client_id)),
            cache: Arc::new(SharedCache::new()),
            current_wallet_type: Arc::new(RwLock::new(None)),
            handles: RwLock::new(Weak::new()),
            init_api_swap: Mutex::new(false),
            expand_event_tx: Arc::new(RwLock::new(None)),
            background_task_pool,
        })
    }

    pub fn get_sn(&self) -> &str {
        &self.sn
    }

    pub fn get_client_id(&self) -> &str {
        &self.client_id
    }

    pub async fn set_frontend_notify_sender(
        &self,
        frontend_notify: FrontendNotifySender,
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut lock = self.frontend_notify.write().await;
        *lock = frontend_notify;
        Ok(())
    }

    pub async fn set_current_wallet_type(
        &self,
        wallet_type: ApiWalletType,
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut lock = self.current_wallet_type.write().await;
        *lock = Some(wallet_type);
        Ok(())
    }

    pub async fn get_current_wallet_type(
        &self,
    ) -> Result<ApiWalletType, crate::error::service::ServiceError> {
        let lock = self.current_wallet_type.read().await;
        (*lock).ok_or_else(|| {
            crate::error::business::BusinessError::Wallet(
                crate::error::business::wallet::WalletError::WalletTypeNotSet,
            )
            .into()
        })
    }

    pub(crate) fn get_global_device(&self) -> Arc<DeviceInfo> {
        self.device.clone()
    }

    pub(crate) fn get_global_sqlite_pool(
        &self,
    ) -> Result<std::sync::Arc<sqlx::SqlitePool>, crate::error::service::ServiceError> {
        let pool = self.core_db.get_pool()?;
        Ok(pool)
    }

    pub(crate) fn core_db(&self) -> &SqliteContext {
        &self.core_db
    }

    pub(crate) fn api_transaction_db(&self) -> &SqliteContext {
        &self.api_transaction_db
    }

    pub(crate) fn api_funds_db(&self) -> &SqliteContext {
        self.api_transaction_db()
    }

    pub(crate) fn api_wallet_db(&self) -> &SqliteContext {
        &self.api_wallet_db
    }

    pub(crate) fn core_pool(
        &self,
    ) -> Result<wallet_database::CoreDbPool, crate::error::service::ServiceError> {
        let pool = self.core_db.get_pool()?;
        Ok(wallet_database::CoreDbPool::new(pool))
    }

    pub(crate) fn api_wallet_pool(
        &self,
    ) -> Result<wallet_database::ApiWalletDbPool, crate::error::service::ServiceError> {
        let pool = self.api_wallet_db.get_pool()?;
        Ok(wallet_database::ApiWalletDbPool::new(pool))
    }

    pub(crate) fn api_transaction_pool(
        &self,
    ) -> Result<wallet_database::ApiTransactionDbPool, crate::error::service::ServiceError> {
        let pool = self.api_transaction_db.get_pool()?;
        Ok(wallet_database::ApiTransactionDbPool::new(pool))
    }

    pub(crate) fn task_db(&self) -> &SqliteContext {
        &self.task_db
    }

    pub(crate) fn task_pool(
        &self,
    ) -> Result<wallet_database::TaskDbPool, crate::error::service::ServiceError> {
        let pool = self.task_db.get_pool()?;
        Ok(wallet_database::TaskDbPool::new(pool))
    }

    pub(crate) fn get_global_backend_api(&self) -> Arc<wallet_transport_backend::api::BackendApi> {
        self.backend_api.clone()
    }

    pub(crate) fn get_api_wallet_backend(&self) -> Arc<dyn ApiWalletBackend> {
        self.api_wallet_backend.clone()
    }

    pub(crate) fn chain_network(&self) -> ChainNetwork {
        self.chain_network
    }

    pub(crate) fn chain_network_kind(&self) -> wallet_types::chain::network::NetworkKind {
        self.chain_network.to_network_kind()
    }

    pub fn get_global_dirs(&self) -> Arc<crate::dirs::Dirs> {
        self.dirs.clone()
    }

    pub(crate) fn get_aggregate_api(&self) -> String {
        self.aggregate_api.clone()
    }

    pub(crate) fn get_global_oss_client(&self) -> Arc<wallet_oss::oss_client::OssClient> {
        self.oss_client.clone()
    }

    pub(crate) fn get_global_cache(&self) -> Arc<SharedCache> {
        self.cache.clone()
    }

    pub(crate) fn get_global_mqtt_topics(&self) -> std::sync::Arc<RwLock<Topics>> {
        self.mqtt_topics.clone()
    }

    pub(crate) fn get_global_frontend_notify_sender(
        &self,
    ) -> std::sync::Arc<RwLock<FrontendNotifySender>> {
        // tracing::info!("context: {:#?}", self);
        // tracing::info!("frontend_notify: {:#?}", self.frontend_notify);
        self.frontend_notify.clone()
    }

    pub(crate) async fn get_rpc_header(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, crate::error::service::ServiceError>
    {
        self.get_rpc_header_with_mode(false).await
    }

    pub(crate) async fn get_rpc_header_force_refresh(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, crate::error::service::ServiceError>
    {
        self.get_rpc_header_with_mode(true).await
    }

    pub(crate) async fn invalidate_rpc_token_cache(&self) {
        let mut token_guard = self.rpc_token.write().await;
        token_guard.token.clear();
        token_guard.instance = tokio::time::Instant::now() - tokio::time::Duration::from_secs(1);
        tracing::warn!(client_id = %self.device.client_id, "rpc token cache invalidated");
    }

    fn rpc_header_with_token(token: String) -> HashMap<String, String> {
        HashMap::from([("token".to_string(), token)])
    }

    async fn get_rpc_header_with_mode(
        &self,
        force_refresh: bool,
    ) -> Result<std::collections::HashMap<String, String>, crate::error::service::ServiceError>
    {
        let token_expired = {
            let token_guard = self.rpc_token.read().await;
            token_guard.token.is_empty() || token_guard.instance < tokio::time::Instant::now()
        };

        if force_refresh || token_expired {
            let _refresh_guard = self.rpc_token_refresh_lock.lock().await;

            if !force_refresh {
                let token_guard = self.rpc_token.read().await;
                let token_still_valid = !token_guard.token.is_empty()
                    && token_guard.instance >= tokio::time::Instant::now();
                if token_still_valid {
                    return Ok(Self::rpc_header_with_token(token_guard.token.clone()));
                }
            }

            let backend_api = self.backend_api.clone();
            tracing::info!(
                client_id = %self.device.client_id,
                force_refresh = force_refresh,
                "rpc token refresh start"
            );
            let new_token_response = backend_api.rpc_token(&self.device.client_id).await;
            match new_token_response {
                Ok(token) => {
                    let new_token = RpcToken {
                        token: token.clone(),
                        instance: tokio::time::Instant::now()
                            + tokio::time::Duration::from_secs(30 * 60),
                    };
                    {
                        let mut token_guard = self.rpc_token.write().await;
                        *token_guard = new_token.clone();
                    }
                    tracing::info!(
                        client_id = %self.device.client_id,
                        force_refresh = force_refresh,
                        "rpc token refresh success"
                    );
                    Ok(Self::rpc_header_with_token(new_token.token))
                }
                Err(e) => {
                    tracing::warn!(
                        client_id = %self.device.client_id,
                        force_refresh = force_refresh,
                        error = %e,
                        "rpc token refresh failed"
                    );
                    if force_refresh {
                        return Err(e.into());
                    }

                    // 服务端报错,如果token有值那么使用原来的值，服务端token的过期时间会大于我本地的。
                    let token_guard = self.rpc_token.read().await;
                    let token = token_guard.token.clone();
                    if !token.is_empty() {
                        Ok(Self::rpc_header_with_token(token))
                    } else {
                        tracing::error!("get_rpc_header failed to get token, error: {:?}", e);
                        Ok(HashMap::new())
                        // Err(crate::error::business::BusinessError::Chain(
                        //     crate::error::business::chain::ChainError::NodeToken(e.to_string()),
                        // ))?
                    }
                }
            }
        } else {
            // 未过期使用缓存里面的token
            let token_guard = self.rpc_token.read().await;
            let token = token_guard.token.clone();
            Ok(Self::rpc_header_with_token(token))
        }
    }

    // 保持原有方法签名以兼容现有代码
    pub(crate) async fn get_global_handles(&self) -> Weak<Handles> {
        self.handles.read().await.clone()
    }

    // 新增方法，返回Result<Arc<Handles>>
    pub(crate) async fn get_handles_arc(
        &self,
    ) -> Result<Arc<Handles>, crate::error::service::ServiceError> {
        self.handles.read().await.upgrade().ok_or_else(|| {
            crate::error::service::ServiceError::System(SystemError::Internal(
                "Handles not initialized or already dropped".to_string(),
            ))
        })
    }

    // pub(crate) async fn get_global_asset_calc_actor_manager(
    //     &self,
    // ) -> Result<Arc<AssetCalcActorManager>, crate::error::service::ServiceError> {
    //     let handles = self.get_handles_arc().await?;
    //     Ok(handles.get_global_asset_calc_actor_manager())
    // }

    pub(crate) async fn set_global_handles(&self, handles: Weak<Handles>) {
        let mut lock = self.handles.write().await;
        *lock = handles;
    }

    pub(crate) async fn is_init_api_swap(&self) -> bool {
        let r = self.init_api_swap.lock().await;
        *r
    }

    pub(crate) async fn set_init_api_swap(&self, swap: bool) {
        let mut r = self.init_api_swap.lock().await;
        *r = swap;
    }

    pub(crate) async fn set_expand_event_tx(&self, tx: Option<ExpandEventSender>) {
        let mut lock = self.expand_event_tx.write().await;
        *lock = tx;
    }

    pub(crate) async fn get_expand_event_tx(&self) -> Option<ExpandEventSender> {
        let lock = self.expand_event_tx.read().await;
        lock.clone()
    }

    /// 获取全局后台任务池
    pub(crate) fn get_global_background_task_pool(&self) -> Arc<BackgroundTaskPool> {
        self.background_task_pool.clone()
    }
}
