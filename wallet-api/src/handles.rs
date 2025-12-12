use crate::{
    context::Context,
    domain::app::{DeviceDomain, config::ConfigDomain},
    infrastructure::{
        self,
        asset_calc::actor_model::AssetCalcActorManager,
        collect::process_collect_tx::ProcessCollectTxHandle,
        collect_fee::process_fee_tx::ProcessFeeTxHandle,
        collector_unconfirm_msg::UnconfirmedMsgCollector,
        inner_event::InnerEventHandle,
        log::upload_log::UploadLogHandle,
        mqtt::{init::ProcessMqttHandle, property::UserProperty},
        private_key_manager::PrivateKeyManager,
        process_unconfirm_msg::UnconfirmedMsgProcessorHandle,
        task_queue::task_manager::TaskManager,
        withdraw::process_withdraw_tx::ProcessWithdrawTxHandle,
    },
};
use std::sync::Arc;
use tokio::sync::Mutex;
use wallet_database::repositories::device::DeviceRepo;

#[derive(Debug)]
pub struct Handles {
    task_manager: Arc<TaskManager>,
    inner_event_handle: Arc<InnerEventHandle>,
    unconfirmed_msg_collector: Arc<UnconfirmedMsgCollector>,
    unconfirmed_msg_processor: Arc<UnconfirmedMsgProcessorHandle>,
    process_withdraw_tx_handle: Arc<ProcessWithdrawTxHandle>,
    process_fee_tx_handle: Arc<ProcessFeeTxHandle>,
    process_collect_tx_handle: Arc<ProcessCollectTxHandle>,
    upload_log: Arc<UploadLogHandle>,
    normal_wallet_mqtt: Arc<Mutex<Option<ProcessMqttHandle>>>,
    api_wallet_mqtt: Arc<Mutex<Option<ProcessMqttHandle>>>,
    asset_calc_actor_manager: Arc<AssetCalcActorManager>,
    private_key_manager: Arc<PrivateKeyManager>,
}

impl Handles {
    pub async fn new(ctx: &'static Context, client_id: &str, pool: Arc<sqlx::SqlitePool>) -> Self {
        let unconfirmed_msg_collector = UnconfirmedMsgCollector::new();
        // 创建 TaskManager 实例
        let notify = Arc::new(tokio::sync::Notify::new());
        let task_manager = TaskManager::new(notify.clone());

        let unconfirmed_msg_processor =
            UnconfirmedMsgProcessorHandle::new(&client_id, notify).await;

        let inner_event_handle = InnerEventHandle::new();

        let process_withdraw_tx_handle = ProcessWithdrawTxHandle::new(ctx, pool.clone()).await;
        let process_fee_tx_handle = ProcessFeeTxHandle::new(ctx, pool.clone()).await;
        let process_collect_tx_handle = ProcessCollectTxHandle::new(pool.clone()).await;

        // 初始化私钥管理器
        tracing::info!("Initialize private key manager start");
        let private_key_manager = Arc::new(
            crate::infrastructure::private_key_manager::PrivateKeyManager::new().await.unwrap(),
        );
        tracing::info!("Initialize private key manager completed");
        let context = crate::context::CONTEXT.get().unwrap();
        let dirs = context.get_global_dirs();
        let base_path = infrastructure::log::format::LogBasePath(dirs.get_log_dir());
        let upload_log_handle =
            UploadLogHandle::new(base_path, 5 * 60, context.get_global_oss_client()).await;
        let asset_calc_actor_manager = AssetCalcActorManager::start(pool.clone());
        Self {
            task_manager: Arc::new(task_manager),
            inner_event_handle: Arc::new(inner_event_handle),
            unconfirmed_msg_collector: Arc::new(unconfirmed_msg_collector),
            unconfirmed_msg_processor: Arc::new(unconfirmed_msg_processor),
            process_withdraw_tx_handle: Arc::new(process_withdraw_tx_handle),
            process_fee_tx_handle: Arc::new(process_fee_tx_handle),
            process_collect_tx_handle: Arc::new(process_collect_tx_handle),
            upload_log: Arc::new(upload_log_handle),
            normal_wallet_mqtt: Arc::new(Mutex::new(None)),
            api_wallet_mqtt: Arc::new(Mutex::new(None)),
            asset_calc_actor_manager: Arc::new(asset_calc_actor_manager),
            private_key_manager: private_key_manager.clone(),
        }
    }

    pub(crate) async fn close(&self) -> Result<(), crate::error::service::ServiceError> {
        self.process_withdraw_tx_handle.close().await?;
        self.process_fee_tx_handle.close().await?;
        self.process_collect_tx_handle.close().await?;
        self.upload_log.close().await?;
        self.unconfirmed_msg_processor.close().await?;
        {
            let mut normal_wallet_mqtt = self.normal_wallet_mqtt.lock().await;
            if let Some(normal_wallet_mqtt) = normal_wallet_mqtt.take() {
                normal_wallet_mqtt.close().await?;
            }
        }
        {
            let mut api_wallet_mqtt = self.api_wallet_mqtt.lock().await;
            if let Some(api_wallet_mqtt) = api_wallet_mqtt.take() {
                api_wallet_mqtt.close().await?;
            }
        }
        // 关闭私钥管理器
        self.private_key_manager.close().await?;
        Ok(())
    }

    pub(crate) fn get_global_processed_withdraw_tx_handle(&self) -> Arc<ProcessWithdrawTxHandle> {
        self.process_withdraw_tx_handle.clone()
    }

    pub(crate) fn get_global_processed_fee_tx_handle(&self) -> Arc<ProcessFeeTxHandle> {
        self.process_fee_tx_handle.clone()
    }

    pub(crate) fn get_global_processed_collect_tx_handle(&self) -> Arc<ProcessCollectTxHandle> {
        self.process_collect_tx_handle.clone()
    }

    pub(crate) fn get_global_task_manager(&self) -> Arc<TaskManager> {
        self.task_manager.clone()
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

    pub(crate) fn get_global_asset_calc_actor_manager(&self) -> Arc<AssetCalcActorManager> {
        self.asset_calc_actor_manager.clone()
    }

    pub(crate) fn get_global_private_key_manager(&self) -> Arc<PrivateKeyManager> {
        self.private_key_manager.clone()
    }

    pub(crate) fn get_global_unconfirmed_msg_processor(
        &self,
    ) -> Arc<UnconfirmedMsgProcessorHandle> {
        self.unconfirmed_msg_processor.clone()
    }

    pub(crate) async fn init_normal_wallet_mqtt(
        &self,
    ) -> Result<(), crate::error::service::ServiceError> {
        let ctx = crate::context::CONTEXT.get().unwrap();
        let pool = ctx.get_global_sqlite_pool()?;
        let Some(device) = DeviceRepo::get_device_info(pool, ctx.get_sn()).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };
        let content = DeviceDomain::device_content(&device)?;
        let client_id = DeviceDomain::client_id_by_device(&device)?;
        let password = DeviceDomain::md5_sn(&device.sn);

        let app_version = ConfigDomain::get_app_version().await?;

        let property =
            UserProperty::new(content, client_id, &device.sn, password, &app_version.app_version);

        let url = ConfigDomain::get_mqtt_uri().await?.ok_or(
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::MqttClientNotInit,
            ),
        )?;
        let h = ProcessMqttHandle::new(property, url).await?;
        self.normal_wallet_mqtt.lock().await.replace(h);
        Ok(())
    }

    pub(crate) async fn init_api_wallet_mqtt(
        &self,
    ) -> Result<(), crate::error::service::ServiceError> {
        let ctx = crate::context::CONTEXT.get().unwrap();
        let pool = ctx.get_global_sqlite_pool()?;
        let Some(device) = DeviceRepo::get_device_info(pool, ctx.get_sn()).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };
        let content = DeviceDomain::device_content(&device)?;
        let client_id = DeviceDomain::client_id_by_device(&device)? + "_aw";
        let password = DeviceDomain::md5_sn(&device.sn);

        let app_version = ConfigDomain::get_app_version().await?;

        let property =
            UserProperty::new(content, client_id, &device.sn, password, &app_version.app_version);

        let url = ConfigDomain::get_mqtt_uri().await?.ok_or(
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::MqttClientNotInit,
            ),
        )?;
        let h = ProcessMqttHandle::new(property, url).await?;
        self.api_wallet_mqtt.lock().await.replace(h);
        Ok(())
    }

    pub(crate) fn get_normal_wallet_mqtt(&self) -> Arc<Mutex<Option<ProcessMqttHandle>>> {
        self.normal_wallet_mqtt.clone()
    }
}
