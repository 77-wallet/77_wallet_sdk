use crate::{
    messaging::notify::FrontendNotifyEvent,
    data::{DeviceInfo, RpcToken},
    dirs::Dirs,
    infrastructure::{
        SharedCache,
        inner_event::InnerEventHandle,
        process_fee_tx::ProcessFeeTxHandle,
        process_unconfirm_msg::{UnconfirmedMsgCollector, UnconfirmedMsgProcessor},
        process_withdraw_tx::ProcessWithdrawTxHandle,
        task_queue::task_manager::TaskManager,
    },
    messaging::mqtt::subscribed::Topics,
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use wallet_database::SqliteContext;

pub type FrontendNotifySender = Option<tokio::sync::mpsc::UnboundedSender<FrontendNotifyEvent>>;

pub(crate) static CONTEXT: once_cell::sync::Lazy<tokio::sync::OnceCell<Context>> =
    once_cell::sync::Lazy::new(tokio::sync::OnceCell::new);

pub(crate) async fn init_context<'a>(
    sn: &str,
    device_type: &str,
    dirs: Dirs,
    frontend_notify: Option<tokio::sync::mpsc::UnboundedSender<FrontendNotifyEvent>>,
    config: crate::config::Config,
) -> Result<&'a Context, crate::error::ServiceError> {
    let context = CONTEXT
        .get_or_try_init::<crate::error::ServiceError, _, _>(|| async {
            let context = Context::new(sn, device_type, dirs, frontend_notify, config).await?;
            Ok(context)
        })
        .await?;

    Ok(context)
}

#[derive(Debug, Clone)]
pub struct Context {
    dirs: Arc<Dirs>,
    aggregate_api: String,
    backend_api: Arc<wallet_transport_backend::api::BackendApi>,
    sqlite_context: Arc<wallet_database::SqliteContext>,
    oss_client: Arc<wallet_oss::oss_client::OssClient>,
    frontend_notify: Arc<RwLock<FrontendNotifySender>>,
    task_manager: Arc<TaskManager>,
    mqtt_topics: Arc<RwLock<Topics>>,
    rpc_token: Arc<RwLock<RpcToken>>,
    device: Arc<DeviceInfo>,
    cache: Arc<SharedCache>,
    inner_event_handle: Arc<InnerEventHandle>,
    unconfirmed_msg_collector: Arc<UnconfirmedMsgCollector>,
    unconfirmed_msg_processor: Arc<UnconfirmedMsgProcessor>,
    process_withdraw_tx_handle: Arc<ProcessWithdrawTxHandle>,
    process_fee_tx_handle: Arc<ProcessFeeTxHandle>,
}

impl Context {
    async fn new(
        sn: &str,
        device_type: &str,
        dirs: Dirs,
        frontend_notify: FrontendNotifySender,
        config: crate::config::Config,
    ) -> Result<Context, crate::error::ServiceError> {
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
            dirs: Arc::new(dirs),
            backend_api: Arc::new(backend_api),
            aggregate_api,
            sqlite_context: Arc::new(sqlite_context),
            frontend_notify,
            oss_client: Arc::new(oss_client),
            task_manager: Arc::new(task_manager),
            mqtt_topics: Arc::new(RwLock::new(Topics::new())),
            rpc_token: Arc::new(RwLock::new(RpcToken::default())),
            device: Arc::new(DeviceInfo::new(sn, &client_id)),
            cache: Arc::new(SharedCache::new()),
            inner_event_handle: Arc::new(inner_event_handle),
            unconfirmed_msg_collector: Arc::new(unconfirmed_msg_collector),
            unconfirmed_msg_processor: Arc::new(unconfirmed_msg_processor),
            process_withdraw_tx_handle: Arc::new(process_withdraw_tx_handle),
            process_fee_tx_handle: Arc::new(process_fee_tx_handle),
        })
    }
    
    pub async fn set_frontend_notify_sender(
        &self,
        frontend_notify: FrontendNotifySender,
    ) -> Result<(), crate::error::ServiceError> {
        let mut lock = self.frontend_notify.write().await;
        *lock = frontend_notify;
        Ok(())
    }

    pub(crate) fn get_global_device(&self) -> Arc<DeviceInfo> {
        self.device.clone()
    }

    pub(crate) fn get_global_sqlite_pool(
        &self,
    ) -> Result<std::sync::Arc<sqlx::SqlitePool>, crate::error::ServiceError> {
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

    pub(crate) fn get_global_task_manager(&self) -> Arc<TaskManager> {
        self.task_manager.clone()
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
    ) -> Result<std::collections::HashMap<String, String>, crate::error::ServiceError> {
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
                        Err(crate::error::business::BusinessError::Chain(crate::error::business::chain::ChainError::NodeToken(
                            e.to_string(),
                        )))?
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

    pub(crate) fn get_global_inner_event_handle(&self) -> Arc<InnerEventHandle> {
        self.inner_event_handle.clone()
    }

    pub(crate) fn get_global_notify(&self) -> Arc<tokio::sync::Notify> {
        self.task_manager.notify.clone()
    }

    pub(crate) fn get_global_unconfirmed_msg_collector(&self) -> Arc<UnconfirmedMsgCollector> {
        self.unconfirmed_msg_collector.clone()
    }

    pub(crate) fn get_global_unconfirmed_msg_processor(&self) -> Arc<UnconfirmedMsgProcessor> {
        self.unconfirmed_msg_processor.clone()
    }

    pub(crate) fn get_global_processed_withdraw_tx_handle(&self) -> Arc<ProcessWithdrawTxHandle> {
        self.process_withdraw_tx_handle.clone()
    }

    pub(crate) fn get_global_processed_fee_tx_handle(&self) -> Arc<ProcessFeeTxHandle> {
        self.process_fee_tx_handle.clone()
    }
}
