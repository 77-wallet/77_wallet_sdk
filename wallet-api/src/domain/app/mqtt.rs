use super::{DeviceDomain, config::ConfigDomain};
use crate::infrastructure::mqtt::{init::init_mqtt_processor, property::UserProperty};
use wallet_database::repositories::device::DeviceRepo;

pub(crate) struct MqttDomain;

impl MqttDomain {
    pub(crate) async fn init() -> Result<(), crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let Some(device) = DeviceRepo::get_device_info(pool).await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };
        let content = DeviceDomain::device_content(&device)?;
        let client_id = DeviceDomain::client_id_by_device(&device)?;
        let password = DeviceDomain::md5_sn(&device.sn);

        let app_version = super::config::ConfigDomain::get_app_version().await?;

        let property =
            UserProperty::new(content, client_id, &device.sn, password, &app_version.app_version);

        let url = ConfigDomain::get_mqtt_uri()
            .await?
            .ok_or(crate::ServiceError::System(crate::SystemError::MqttClientNotInit))?;
        init_mqtt_processor(property, url).await?;

        Ok(())
    }

    pub(crate) async fn process_unconfirm_msg(client_id: &str) -> Result<(), crate::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let data = backend_api
            .query_unconfirm_msg(&wallet_transport_backend::request::QueryUnconfirmMsgReq {
                client_id: client_id.to_string(),
            })
            .await?
            .list;
        tracing::debug!("query_unconfirm_msg: {}", data.len());
        crate::service::jpush::JPushService::jpush_multi(
            data,
            // MsgConfirmSource::Api
        )
        .await?;
        // TODO: 目前任务执行完后，会自动发送 send_msg_confirm，所以这里不需要再发送
        // crate::domain::task_queue::TaskQueueDomain::send_msg_confirm(ids).await?;
        Ok(())
    }
}
