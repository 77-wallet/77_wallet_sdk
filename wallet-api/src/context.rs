use crate::{
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
use futures::{FutureExt, StreamExt};
use sqlx::__rt::sleep;
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use wallet_database::{SqliteContext, entities::api_wallet::ApiWalletType};

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

#[derive(Debug)]
pub struct Context {
    sn: String,
    client_id: String,
    dirs: Arc<Dirs>,
    aggregate_api: String,
    backend_api: Arc<wallet_transport_backend::api::BackendApi>,
    core_db: Arc<wallet_database::SqliteContext>, // data.db
    api_wallet_db: Arc<wallet_database::SqliteContext>, // api_wallet.db
    api_funds_db: Arc<wallet_database::SqliteContext>, // api_funds.db
    task_db: Arc<wallet_database::SqliteContext>, // task.db
    oss_client: Arc<wallet_oss::oss_client::OssClient>,
    frontend_notify: Arc<RwLock<FrontendNotifySender>>,
    mqtt_topics: Arc<RwLock<Topics>>,
    rpc_token: Arc<RwLock<RpcToken>>,
    device: Arc<DeviceInfo>,
    cache: Arc<SharedCache>,
    current_wallet_type: Arc<RwLock<ApiWalletType>>,
    handles: RwLock<Weak<Handles>>,
    init_api_swap: Mutex<bool>,
    locks: Mutex<HashMap<String, bool>>,
    wallet_seeds: Arc<RwLock<HashMap<String, Vec<u8>>>>,
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
        let api_funds_db = SqliteContext::new(db_path, Some("api_funds.db")).await?;
        let task_db = SqliteContext::new(db_path, Some("task.db")).await?;

        let client_id = crate::domain::app::DeviceDomain::client_device_by_sn(sn, device_type);
        tracing::info!(" ======================================  client id: {}", client_id);

        #[cfg(feature = "dev")]
        let api_url = config.backend_api.dev_url;
        #[cfg(feature = "test")]
        let api_url = config.backend_api.test_url;
        #[cfg(feature = "prod")]
        let api_url = config.backend_api.prod_url;

        // 聚合器api
        #[cfg(feature = "dev")]
        let aggregate_api = config.aggregate_api.dev_url;
        #[cfg(feature = "test")]
        let aggregate_api = config.aggregate_api.test_url;
        #[cfg(feature = "prod")]
        let aggregate_api = config.aggregate_api.prod_url;

        tracing::info!("api_url: {}, client_id: {}", api_url, client_id);
        let mut headers_opt = HashMap::new();
        headers_opt.insert("clientId".to_string(), client_id.clone());
        headers_opt.insert("AW-SEC-ID".to_string(), sn.to_string());
        let aes_cbc_cryptor =
            wallet_utils::cbc::AesCbcCryptor::new(&config.crypto.aes_key, &config.crypto.aes_iv);
        let backend_api = wallet_transport_backend::api::BackendApi::new(
            Some(api_url.to_string()),
            Some(headers_opt),
            aes_cbc_cryptor,
        )?;

        let frontend_notify = Arc::new(RwLock::new(frontend_notify));

        {
            let mut app_state = crate::app_state::APP_STATE.write().await;
            app_state.set_backend_url(Some(backend_api.base_url.clone()));
        }

        let oss_client = wallet_oss::oss_client::OssClient::new(&config.oss);

        // 创建后台任务池：SQLite连接池有限，后台任务并发过高会放大“pool timed out”概率
        // 可按需调大，但建议与各DB的max_connections同量级。
        let background_task_pool = Arc::new(BackgroundTaskPool::new(32));

        Ok(Context {
            sn: sn.to_string(),
            client_id: client_id.clone(),
            dirs: Arc::new(dirs),
            backend_api: Arc::new(backend_api),
            aggregate_api,
            core_db: Arc::new(core_db),
            api_wallet_db: Arc::new(api_wallet_db),
            api_funds_db: Arc::new(api_funds_db),
            task_db: Arc::new(task_db),
            frontend_notify,
            oss_client: Arc::new(oss_client),
            mqtt_topics: Arc::new(RwLock::new(Topics::new())),
            rpc_token: Arc::new(RwLock::new(RpcToken::default())),
            device: Arc::new(DeviceInfo::new(sn, &client_id)),
            cache: Arc::new(SharedCache::new()),
            current_wallet_type: Arc::new(RwLock::new(ApiWalletType::InvalidValue)),
            handles: RwLock::new(Weak::new()),
            init_api_swap: Mutex::new(false),
            locks: Mutex::new(HashMap::new()),
            wallet_seeds: Arc::new(RwLock::new(HashMap::new())),
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
        *lock = wallet_type;
        Ok(())
    }

    pub async fn get_current_wallet_type(&self) -> ApiWalletType {
        let lock = self.current_wallet_type.read().await;
        *lock
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

    pub(crate) fn api_funds_db(&self) -> &SqliteContext {
        &self.api_funds_db
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

    pub(crate) fn api_funds_pool(
        &self,
    ) -> Result<wallet_database::CollectDbPool, crate::error::service::ServiceError> {
        let pool = self.api_funds_db.get_pool()?;
        Ok(wallet_database::CollectDbPool::new(pool))
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
        let token_expired = {
            let token_guard = self.rpc_token.read().await;
            token_guard.instance < tokio::time::Instant::now()
        };

        if token_expired {
            let backend_api = self.backend_api.clone();
            let new_token_response = backend_api.rpc_token(&self.device.client_id).await;
            match new_token_response {
                Ok(token) => {
                    let new_token = RpcToken {
                        token,
                        instance: tokio::time::Instant::now()
                            + tokio::time::Duration::from_secs(30 * 60),
                    };
                    {
                        let mut token_guard = self.rpc_token.write().await;
                        *token_guard = new_token.clone();
                    }
                    Ok(HashMap::from([("token".to_string(), new_token.token)]))
                }
                Err(e) => {
                    // 服务端报错,如果token有值那么使用原来的值，服务端token的过期时间会大于我本地的。

                    let token_guard = self.rpc_token.read().await;
                    let token = token_guard.token.clone();
                    if !token.is_empty() {
                        Ok(HashMap::from([("token".to_string(), token)]))
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

            Ok(HashMap::from([("token".to_string(), token)]))
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

    pub(crate) async fn lock_account(&self, account: &str) {
        loop {
            let mut l = self.locks.lock().await;
            let acccount = l.get(account);
            match acccount {
                Some(lock) => {
                    if *(lock) {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    } else {
                        l.insert(account.to_string(), true);
                        break;
                    }
                }
                None => {
                    l.insert(account.to_string(), true);
                    break;
                }
            }
        }
    }

    pub(crate) async fn unlock_account(&self, account: &str) {
        let mut l = self.locks.lock().await;
        l.insert(account.to_string(), false);
    }

    pub(crate) async fn seed_list(&self) -> Vec<String> {
        let lock = self.wallet_seeds.read().await;
        lock.keys().cloned().collect()
    }

    pub(crate) async fn is_wallet_seed_set(&self, uid: &str) -> bool {
        let lock = self.wallet_seeds.read().await;
        lock.contains_key(uid)
    }

    pub(crate) async fn get_wallet_seed(&self, uid: &str) -> Option<Vec<u8>> {
        let lock = self.wallet_seeds.read().await;
        lock.get(uid).cloned()
    }

    pub(crate) async fn set_wallet_seed(&self, uid: &str, seed: &[u8]) {
        let mut lock = self.wallet_seeds.write().await;
        lock.insert(uid.to_string(), seed.to_vec());
    }

    pub(crate) async fn remove_wallet_seed(&self, uid: &[String]) {
        let mut lock = self.wallet_seeds.write().await;
        for u in uid {
            lock.remove(u.as_str());
        }
    }

    pub(crate) async fn clear_wallet_seed(&self) {
        let mut lock = self.wallet_seeds.write().await;
        lock.clear();
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
