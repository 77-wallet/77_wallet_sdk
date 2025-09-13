use wallet_database::entities::task_queue::TaskQueueEntity;
// use wallet_transport_backend::request::MsgConfirmSource;
// use wallet_transport_backend::request::MsgConfirmSource;
use wallet_utils::serde_func;

use crate::messaging::{mqtt::Message, notify::FrontendNotifyEvent};

pub struct JPushService {}

impl JPushService {
    // 前端发来的消息
    pub async fn jpush(message: &str) -> Result<(), crate::ServiceError> {
        // Self::jpush_multi(vec![message.to_string()], "JG").await?;
        match serde_func::serde_from_str::<Message>(message) {
            Ok(data) => {
                let backend_api =
                    crate::context::CONTEXT.get().unwrap().get_global_backend_api();

                // 重新查询一次,前端给到的数据不全面
                let data = backend_api
                    .get_unconfirm_by_msg_id(&wallet_transport_backend::request::GetUnconfirmById {
                        msg_id: data.msg_id.to_string(),
                    })
                    .await?;

                if let Some(msg) = data.body {
                    Self::jpush_multi(
                        vec![msg],
                        // MsgConfirmSource::Jg
                    )
                    .await?;
                };
            }
            Err(e) => {
                tracing::error!("[jpush] serde_from_str error: {}", e);
                if let Err(e) = FrontendNotifyEvent::send_error("jpush", e.to_string()).await {
                    tracing::error!("send_error error: {}", e);
                }
            }
        };

        Ok(())
    }

    pub async fn jpush_multi(
        messages: Vec<String>,
        // source: MsgConfirmSource,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let unconfirmed_msg_collector =
            crate::context::CONTEXT.get().unwrap().get_global_unconfirmed_msg_collector();
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
            if let Some(task_entity) =
                TaskQueueEntity::get_task_queue(pool.as_ref(), &payload.msg_id).await?
                && task_entity.status == 2
            {
                unconfirmed_msg_collector.submit(vec![id])?;
            } else if let Err(e) = crate::messaging::mqtt::handle::exec_payload(payload).await {
                if let Err(e) = FrontendNotifyEvent::send_error("jpush_multi", e.to_string()).await
                {
                    tracing::error!("send_error error: {}", e);
                }
                tracing::error!("[jpush_multi] exec_payload error: {}", e);
            };
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::messaging::mqtt::{Message, topics::OrderMultiSignAcceptCompleteMsg};

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
