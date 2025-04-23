use wallet_database::entities::task_queue::TaskQueueEntity;
use wallet_transport_backend::{consts::endpoint::SEND_MSG_CONFIRM, request::SendMsgConfirmReq};
use wallet_utils::serde_func;

use crate::{
    infrastructure::task_queue::{BackendApiTask, Task, Tasks},
    messaging::{mqtt::Message, notify::FrontendNotifyEvent},
};

pub struct JPushService {}

impl JPushService {
    pub async fn jpush(_message: &str) -> Result<(), crate::ServiceError> {
        // todo : 有些消息数据过大导致极光推送失败，缩减消息体后数据格式不匹配
        // 现在不做处理等待 pull_confirm 去获取。
        // Self::jpush_multi(vec![message.to_string()], "JG").await?;
        Ok(())
    }

    pub async fn jpush_multi(
        messages: Vec<String>,
        source: &str,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let mut ids = Vec::new();
        for message in messages {
            let payload = match serde_func::serde_from_str::<Message>(message.as_str()) {
                Ok(data) => data,
                Err(e) => {
                    tracing::error!("[jpush_multi] serde_from_str error: {}", e);
                    if let Err(e) =
                        FrontendNotifyEvent::send_error("jpush_multi", e.to_string()).await
                    {
                        tracing::error!("send_error error: {}", e);
                    }
                    continue;
                }
            };
            let id = payload.msg_id.clone();
            if TaskQueueEntity::get_task_queue(pool.as_ref(), &payload.msg_id)
                .await?
                .is_none()
            {
                if let Err(e) = crate::messaging::mqtt::handle::exec_payload(payload).await {
                    if let Err(e) =
                        FrontendNotifyEvent::send_error("jpush_multi", e.to_string()).await
                    {
                        tracing::error!("send_error error: {}", e);
                    }
                    tracing::error!("[jpush_multi] exec_payload error: {}", e);
                };
            };
            ids.push(wallet_transport_backend::request::SendMsgConfirm::new(
                &id, source,
            ));
        }
        if !ids.is_empty() {
            let send_msg_confirm_req =
                BackendApiTask::new(SEND_MSG_CONFIRM, &SendMsgConfirmReq::new(ids))?;
            Tasks::new()
                .push(Task::BackendApi(send_msg_confirm_req))
                .send()
                .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::messaging::mqtt::{topics::OrderMultiSignAcceptCompleteMsg, Message};

    #[test]
    fn test_() {
        let message = "{\"clientId\":\"wenjing\",\"sn\":\"device457\",\"deviceType\":\"ANDROID\",\"bizType\":\"ORDER_MULTI_SIGN_ACCEPT_COMPLETE_MSG\",\"body\":{\"status\":1,\"multisigAccountId\":\"order-1\",\"addressList\":[],\"acceptStatus\":false,\"acceptAddressList\":[\"THx9ao6pdLUFoS3CSc98pwj1HCrmGHoVUB\"]}}";

        let _payload: Message = wallet_utils::serde_func::serde_from_str(message).unwrap();
    }

    #[test]
    fn test_2() {
        let message = "{\"status\":1,\"multisigAccountId\":\"order-1\",\"addressList\":[],\"acceptStatus\":false,\"acceptAddressList\":[\"THx9ao6pdLUFoS3CSc98pwj1HCrmGHoVUB\"]}";

        let _payload: OrderMultiSignAcceptCompleteMsg =
            wallet_utils::serde_func::serde_from_str(message).unwrap();
    }
}
