use wallet_database::entities::task_queue::TaskQueueEntity;
// use wallet_transport_backend::request::MsgConfirmSource;
use wallet_utils::serde_func;

use crate::messaging::{mqtt::Message, notify::FrontendNotifyEvent};

pub struct JPushService {}

impl JPushService {
    pub async fn jpush(message: &str) -> Result<(), crate::ServiceError> {
        // Self::jpush_multi(vec![message.to_string()], "JG").await?;
        match serde_func::serde_from_str::<Message>(message) {
            Ok(data) => {
                let backend_api = crate::manager::Context::get_global_backend_api()?;
                let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;

                let data = backend_api
                    .get_unconfirm_by_msg_id(
                        cryptor,
                        &wallet_transport_backend::request::GetUnconfirmById {
                            msg_id: data.msg_id.to_string(),
                        },
                    )
                    .await?;

                if let Some(msg) = data.body {
                    Self::jpush_multi(
                        vec![msg],
                        // MsgConfirmSource::Jg
                    )
                    .await?;
                    // TODO: 目前任务执行完后，会自动发送 send_msg_confirm，所以这里不需要再发送
                    // if !ids.is_empty() {
                    //     let send_msg_confirm_req =
                    //         BackendApiTask::new(SEND_MSG_CONFIRM, &SendMsgConfirmReq::new(ids))?;
                    //     Tasks::new()
                    //         .push(Task::BackendApi(send_msg_confirm_req))
                    //         .send()
                    //         .await?;
                    // }
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
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let unconfirmed_msg_collector =
            crate::manager::Context::get_global_unconfirmed_msg_collector()?;
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
            } else {
                if let Err(e) = crate::messaging::mqtt::handle::exec_payload(payload).await {
                    if let Err(e) =
                        FrontendNotifyEvent::send_error("jpush_multi", e.to_string()).await
                    {
                        tracing::error!("send_error error: {}", e);
                    }
                    tracing::error!("[jpush_multi] exec_payload error: {}", e);
                };
            };
            // ids.push(wallet_transport_backend::request::SendMsgConfirm::new(
            //     &id,
            //     source.clone(),
            // ));
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
