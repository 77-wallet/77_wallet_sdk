use super::{DeviceDomain, config::ConfigDomain};
use crate::infrastructure::mqtt::{init::ProcessMqttHandle, property::UserProperty};
use wallet_database::repositories::device::DeviceRepo;

pub(crate) struct MqttDomain {
    h: ProcessMqttHandle,
}

impl MqttDomain {
    pub(crate) async fn init() -> Result<(), crate::error::service::ServiceError> {
        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles();
        if let Some(handles) = handles.upgrade() {
            handles.init_normal_wallet_mqtt().await;
        }
        Ok(())
    }

    pub(crate) async fn process_unconfirm_msg(
        client_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let req = wallet_transport_backend::request::QueryUnconfirmMsgReq {
            client_id: client_id.to_string(),
        };
        let data = backend_api.query_unconfirm_msg(&req).await?.list;
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
