use crate::{
    Dirs, FrontendNotifyEvent,
    infrastructure::{
        SharedCache,
        inner_event::InnerEventHandle,
        process_fee_tx::ProcessFeeTxHandle,
        process_unconfirm_msg::{UnconfirmedMsgCollector, UnconfirmedMsgProcessor},
        process_withdraw_tx::ProcessWithdrawTxHandle,
        task_queue::task_manager::TaskManager,
    },
    manager::{DeviceInfo, FrontendNotifySender, RpcToken},
    messaging::mqtt::subscribed::Topics,
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use wallet_database::SqliteContext;

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
