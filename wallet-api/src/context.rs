use crate::{
    data::{DeviceInfo, RpcToken},
    dirs::Dirs,
    handles::Handles,
    infrastructure::{
        SharedCache,
        process_fee_tx::ProcessFeeTxHandle,
        process_unconfirm_msg::{UnconfirmedMsgCollector, UnconfirmedMsgProcessor},
        task_queue::task_manager::TaskManager,
    },
    messaging::{mqtt::subscribed::Topics, notify::FrontendNotifyEvent},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};
use tokio::sync::RwLock;
use tracing::log;
use wallet_database::{SqliteContext, entities::api_wallet::ApiWalletType};

pub type FrontendNotifySender = Option<tokio::sync::mpsc::UnboundedSender<FrontendNotifyEvent>>;

pub(crate) static CONTEXT: once_cell::sync::Lazy<tokio::sync::OnceCell<Context>> =
    once_cell::sync::Lazy::new(tokio::sync::OnceCell::new);

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
    sn : String,
    client_id: String,
    dirs: Arc<Dirs>,
    aggregate_api: String,
    backend_api: Arc<wallet_transport_backend::api::BackendApi>,
    sqlite_context: Arc<wallet_database::SqliteContext>,
    oss_client: Arc<wallet_oss::oss_client::OssClient>,
    frontend_notify: Arc<RwLock<FrontendNotifySender>>,
    mqtt_topics: Arc<RwLock<Topics>>,
    rpc_token: Arc<RwLock<RpcToken>>,
    device: Arc<DeviceInfo>,
    cache: Arc<SharedCache>,
    current_wallet_type: Arc<RwLock<ApiWalletType>>,
    handles: Mutex<Weak<Handles>>,
}

impl Context {
    async fn new(
        sn: &str,
        device_type: &str,
        dirs: Dirs,
        frontend_notify: FrontendNotifySender,
        config: crate::config::Config,
    ) -> Result<Context, crate::error::service::ServiceError> {
        let sqlite_context = SqliteContext::new(&dirs.db_dir.to_string_lossy()).await?;

        let client_id = crate::domain::app::DeviceDomain::client_device_by_sn(sn, device_type);

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

        log::info!("api_url: {}, client_id: {}", api_url, client_id);
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

        Ok(Context {
            sn: sn.to_string(),
            client_id : client_id.clone(),
            dirs: Arc::new(dirs),
            backend_api: Arc::new(backend_api),
            aggregate_api,
            sqlite_context: Arc::new(sqlite_context),
            frontend_notify,
            oss_client: Arc::new(oss_client),
            mqtt_topics: Arc::new(RwLock::new(Topics::new())),
            rpc_token: Arc::new(RwLock::new(RpcToken::default())),
            device: Arc::new(DeviceInfo::new(sn, &client_id)),
            cache: Arc::new(SharedCache::new()),
            current_wallet_type: Arc::new(RwLock::new(ApiWalletType::InvalidValue)),
            handles: Mutex::new(Weak::new()),
        })
    }

    pub fn get_sn(&self) ->&str {
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
        let pool = self.sqlite_context.get_pool()?;
        Ok(pool)
    }

    pub(crate) fn get_global_backend_api(&self) -> Arc<wallet_transport_backend::api::BackendApi> {
        self.backend_api.clone()
    }

    pub(crate) fn get_global_dirs(&self) -> Arc<crate::dirs::Dirs> {
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

    pub(crate) fn get_global_handles(&self) -> Weak<Handles> {
        self.handles.lock().unwrap().clone()
    }

    pub(crate) fn set_global_handles(&self, handles: Weak<Handles>) {
        let mut lock = self.handles.lock().unwrap();
        *lock = handles;
    }
}
