use rumqttc::v5::mqttbytes::v5::{Packet, Publish};
use wallet_database::{entities::task_queue::TaskQueueEntity, factory::RepositoryFactory};
use wallet_utils::serde_func;

use super::{
    Message,
    message::BizType,
    topics::{
        AcctChange, BulletinMsg, ChainChange, CleanPermission, MultiSignTransAccept,
        MultiSignTransAcceptCompleteMsg, MultiSignTransCancel, MultiSignTransExecute,
        OrderAllConfirmed, OrderMultiSignAccept, OrderMultiSignAcceptCompleteMsg,
        OrderMultiSignCancel, OrderMultiSignCreated, OrderMultiSignServiceComplete,
        PermissionAccept, RpcChange, Topic,
    },
};
use crate::{
    infrastructure::task_queue::{MqttTask, task::Tasks},
    messaging::{
        mqtt::topics::{
            OutgoingPayload,
            api_wallet::{AddressUseMsg, UnbindUidMsg},
        },
        notify::{FrontendNotifyEvent, event::NotifyEvent},
    },
    service::{app::AppService, device::DeviceService},
};

pub(crate) async fn exec_incoming(
    client: &rumqttc::v5::AsyncClient,
    packet: Packet,
) -> Result<(), Box<dyn std::error::Error>> {
    match packet {
        Packet::ConnAck(conn_ack) => {
            exec_incoming_connack(client, conn_ack).await?;
        }
        Packet::Publish(publish) => {
            exec_incoming_publish(&publish).await?;
            client.ack(&publish).await?;
        }
        Packet::PingResp(_) => {
            let data = NotifyEvent::KeepAlive;
            if let Err(e) = FrontendNotifyEvent::new(data).send().await {
                tracing::error!("[exec_incoming] send error: {e}");
            }
        }
        Packet::Disconnect(_) => {
            let data = NotifyEvent::MqttDisconnected;
            FrontendNotifyEvent::new(data).send().await?;
        }
        _ => {}
    }

    Ok(())
}

pub async fn exec_incoming_publish(publish: &Publish) -> Result<(), anyhow::Error> {
    let pool = crate::manager::Context::get_global_sqlite_pool()?;

    let topic = Topic::from_bytes_v3(publish.topic.to_vec())?;

    match topic.topic {
        Topic::Switch => {}
        #[cfg(feature = "token")]
        crate::messaging::mqtt::topics::Topic::Token => {
            let payload: crate::messaging::mqtt::topics::TokenPriceChange =
                serde_json::from_slice(&publish.payload)?;
            payload.exec().await?;
        }
        Topic::RpcChange => {
            let payload: RpcChange = serde_json::from_slice(&publish.payload)?;

            if let Err(e) = FrontendNotifyEvent::send_debug(&payload).await {
                tracing::error!("[exec_incoming_publish] send debug error: {e}");
            };

            payload.exec().await?;
        }
        Topic::ChainChange => {
            let payload: ChainChange = serde_json::from_slice(&publish.payload)?;
            payload.exec().await?;
        }
        Topic::Order | Topic::Common | Topic::BulletinInfo => {
            let payload: Message = serde_json::from_slice(&publish.payload)?;
            if let Err(e) = FrontendNotifyEvent::send_debug(&payload).await {
                tracing::error!("[exec_incoming_publish] send debug error: {e}");
            };

            // TODO: 目前任务执行完后，会自动发送 send_msg_confirm，所以这里不需要再发送
            // let send_msg_confirm_req = BackendApiTask::new(
            //     SEND_MSG_CONFIRM,
            //     &SendMsgConfirmReq::new(vec![SendMsgConfirm::new(
            //         &payload.msg_id,
            //         MsgConfirmSource::Mqtt,
            //     )]),
            // )?;
            // Tasks::new()
            //     .push(Task::BackendApi(send_msg_confirm_req))
            //     .send()
            //     .await?;

            // 是否有相同的队列
            if TaskQueueEntity::get_task_queue(pool.as_ref(), &payload.msg_id).await?.is_none() {
                let event = serde_func::serde_to_string(&payload.biz_type)?;
                if let Err(e) = exec_payload(payload).await {
                    tracing::error!("exec_payload error: {}", e);
                    if let Err(e) = FrontendNotifyEvent::send_error(&event, e.to_string()).await {
                        tracing::error!("send_error error: {}", e);
                    }
                };
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) async fn exec_payload(payload: Message) -> Result<(), crate::ServiceError> {
    match payload.biz_type {
        BizType::OrderMultiSignAccept => {
            exec_task::<OrderMultiSignAccept, _>(&payload, MqttTask::OrderMultiSignAccept).await?
        }
        BizType::OrderMultiSignAcceptCompleteMsg => {
            exec_task::<OrderMultiSignAcceptCompleteMsg, _>(
                &payload,
                MqttTask::OrderMultiSignAcceptCompleteMsg,
            )
            .await?
        }
        BizType::OrderMultiSignServiceComplete => {
            exec_task::<OrderMultiSignServiceComplete, _>(
                &payload,
                MqttTask::OrderMultiSignServiceComplete,
            )
            .await?
        }
        BizType::OrderMultiSignCancel => {
            exec_task::<OrderMultiSignCancel, _>(&payload, MqttTask::OrderMultiSignCancel).await?
        }
        BizType::MultiSignTransAccept => {
            exec_task::<MultiSignTransAccept, _>(&payload, MqttTask::MultiSignTransAccept).await?
        }
        BizType::MultiSignTransAcceptCompleteMsg => {
            exec_task::<MultiSignTransAcceptCompleteMsg, _>(
                &payload,
                MqttTask::MultiSignTransAcceptCompleteMsg,
            )
            .await?
        }
        BizType::AcctChange => exec_task::<AcctChange, _>(&payload, MqttTask::AcctChange).await?,
        BizType::OrderMultiSignCreated => {
            exec_task::<OrderMultiSignCreated, _>(&payload, MqttTask::OrderMultiSignCreated).await?
        }
        BizType::BulletinMsg => {
            exec_task::<BulletinMsg, _>(&payload, MqttTask::BulletinMsg).await?
        }
        BizType::MultiSignTransCancel => {
            exec_task::<MultiSignTransCancel, _>(&payload, MqttTask::MultiSignTransCancel).await?
        }
        BizType::PermissionAccept => {
            exec_task::<PermissionAccept, _>(&payload, MqttTask::PermissionAccept).await?
        }
        BizType::MultiSignTransExecute => {
            exec_task::<MultiSignTransExecute, _>(&payload, MqttTask::MultiSignTransExecute).await?
        }
        BizType::OrderMultiSignAllMemberAccepted => {
            exec_task::<OrderAllConfirmed, _>(&payload, MqttTask::OrderAllConfirmed).await?
        }
        BizType::CleanPermission => {
            exec_task::<CleanPermission, _>(&payload, MqttTask::CleanPermission).await?
        }
        BizType::UnbindUid => exec_task::<UnbindUidMsg, _>(&payload, MqttTask::UnbindUid).await?,
        BizType::AddressUse => {
            exec_task::<AddressUseMsg, _>(&payload, MqttTask::AddressUse).await?
        }
        // 如果没有匹配到任何已知的 BizType，则返回错误
        biztype => {
            return Err(crate::ServiceError::System(crate::SystemError::MessageWrong(
                biztype,
                payload.body,
            )));
        }
    }

    Ok(())
}

async fn exec_task<T, F>(payload: &Message, task_ctor: F) -> Result<(), crate::ServiceError>
where
    T: serde::de::DeserializeOwned,
    F: FnOnce(T) -> MqttTask,
{
    let data = serde_func::serde_from_value::<T>(payload.body.clone())?;
    Tasks::new()
        .push_with_id(
            &payload.msg_id,
            task_ctor(data), // Task::Mqtt(Box::new())
        )
        .send()
        .await?;
    Ok(())
}

async fn exec_incoming_connack(
    client: &rumqttc::v5::AsyncClient,
    conn_ack: rumqttc::v5::mqttbytes::v5::ConnAck,
) -> Result<(), anyhow::Error> {
    let pool = crate::manager::Context::get_global_sqlite_pool()?;
    let repo = RepositoryFactory::repo(pool.clone());
    let device_service = DeviceService::new(repo);

    if conn_ack.code == rumqttc::v5::mqttbytes::v5::ConnectReturnCode::Success {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        AppService::new(repo).mqtt_resubscribe().await?;
    }

    use wallet_database::repositories::wallet::WalletRepoTrait as _;
    let mut repo = RepositoryFactory::repo(pool);
    if let Some(wallet) = repo.wallet_latest().await?
        && let Some(device) = &device_service.get_device_info().await?
        && let Some(app_id) = &device.app_id
    {
        let client_id = crate::domain::app::DeviceDomain::client_id_by_device(device)?;
        let body = OutgoingPayload::SwitchWallet {
            uid: wallet.uid,
            sn: device.sn.clone(),
            app_id: app_id.to_string(),
            device_type: device.device_type.clone(),
            client_id,
        };
        client
            .publish(Topic::Switch, rumqttc::v5::mqttbytes::QoS::AtLeastOnce, false, body.to_vec()?)
            .await?;
    }

    let data = NotifyEvent::MqttConnected;
    FrontendNotifyEvent::new(data).send().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{messaging::mqtt::handle::exec_incoming_publish, test::env::get_manager};

    #[tokio::test]
    async fn test_multi_signature_transfer_is_successful() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "msgId": "1699855163567",
            "appId": "170976fa8a0772b7e2c",
            "bizType": "ACCT_CHANGE",
            "body": {
                "blockHeight": 67090948,
                "chainCode": "tron",
                "fromAddr": "TBk86hq1e8C1gNX6RDXhk1wLamwzKnotmo",
                "isMultisig": 1,
                "notes": "时间",
                "queueId": "197131136378474496",
                "status": true,
                "symbol": "trx",
                "toAddr": "TAqUJ9enU8KkZYySA51iQim7TxbbdLR2wn",
                "token": "",
                "transactionFee": 1.401,
                "transactionTime": "2024-11-18 09:36:51",
                "transferType": 1,
                "txHash": "d92dbe05593f0c6a5e9dd76ecfb181903ffc8de77c43fa50fd4a6f53042c3371",
                "txKind": 1,
                "value": 1
            },
            "clientId": "dc09781f5671e2bc244ba492b8cfb0af",
            "deviceType": "ANDROID",
            "sn": "9580b55ec4a1d3d3af85077ae0c4c901885b1123e50f830cbd5bfbbe0cb161a3"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_multi_signature_transfer_receive_is_successful() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "msgId": "11111111",
            "appId": "170976fa8a0772b7e2c",
            "bizType": "ACCT_CHANGE",
            "body": {
                "blockHeight": 67090948,
                "chainCode": "tron",
                "fromAddr": "TAqUJ9enU8KkZYySA51iQim7TxbbdLR2wn",
                "isMultisig": 0,
                "notes": "时间",
                "queueId": "197131136378474496",
                "status": true,
                "symbol": "trx",
                "toAddr": "TBk86hq1e8C1gNX6RDXhk1wLamwzKnotmo",
                "token": "",
                "transactionFee": 1.401,
                "transactionTime": "2024-11-18 09:36:51",
                "transferType": 0,
                "txHash": "d92dbe05593f0c6a5e9dd76ecfb181903ffc8de77c43fa50fd4a6f53042c3371",
                "txKind": 1,
                "value": 1
            },
            "clientId": "dc09781f5671e2bc244ba492b8cfb0af",
            "deviceType": "ANDROID",
            "sn": "9580b55ec4a1d3d3af85077ae0c4c901885b1123e50f830cbd5bfbbe0cb161a3"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_multi_signature_transfer_to_partner_is_successful() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "msgId": "222222",
            "appId": "170976fa8a0772b7e2c",
            "bizType": "ACCT_CHANGE",
            "body": {
                "blockHeight": 67090948,
                "chainCode": "tron",
                "fromAddr": "TBk86hq1e8C1gNX6RDXhk1wLamwzKnotmo",
                "isMultisig": 1,
                "notes": "时间",
                "queueId": "197131136378474496",
                "status": true,
                "symbol": "trx",
                "toAddr": "TXBLuUhnfofYZAHZTWxYiDjgoaYLtwPzw3",
                "token": "",
                "transactionFee": 1.401,
                "transactionTime": "2024-11-18 09:36:51",
                "transferType": 1,
                "txHash": "d92dbe05593f0c6a5e9dd76ecfb181903ffc8de77c43fa50fd4a6f53042c3371",
                "txKind": 1,
                "value": 1
            },
            "clientId": "dc09781f5671e2bc244ba492b8cfb0af",
            "deviceType": "ANDROID",
            "sn": "9580b55ec4a1d3d3af85077ae0c4c901885b1123e50f830cbd5bfbbe0cb161a3"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_multi_signature_transfer_to_partner_receive_is_successful() -> anyhow::Result<()>
    {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "msgId": "333333333",
            "appId": "170976fa8a0772b7e2c",
            "bizType": "ACCT_CHANGE",
            "body": {
                "blockHeight": 67090948,
                "chainCode": "tron",
                "fromAddr": "TBk86hq1e8C1gNX6RDXhk1wLamwzKnotmo",
                "isMultisig": 1,
                "notes": "时间",
                "queueId": "197131136378474496",
                "status": true,
                "symbol": "trx",
                "toAddr": "TXBLuUhnfofYZAHZTWxYiDjgoaYLtwPzw3",
                "token": "",
                "transactionFee": 1.401,
                "transactionTime": "2024-11-18 09:36:51",
                "transferType": 0,
                "txHash": "d92dbe05593f0c6a5e9dd76ecfb181903ffc8de77c43fa50fd4a6f53042c3371",
                "txKind": 1,
                "value": 1
            },
            "clientId": "dc09781f5671e2bc244ba492b8cfb0af",
            "deviceType": "ANDROID",
            "sn": "9580b55ec4a1d3d3af85077ae0c4c901885b1123e50f830cbd5bfbbe0cb161a3"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_common_transfer_to_multi_is_successful() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "msgId": "123123",
            "appId": "170976fa8a0772b7e2c",
            "bizType": "ACCT_CHANGE",
            "body": {
                "blockHeight": 67090948,
                "chainCode": "tron",
                "fromAddr": "TXBLuUhnfofYZAHZTWxYiDjgoaYLtwPzw3",
                "isMultisig": 0,
                "notes": "时间",
                "queueId": "197131136378474496",
                "status": true,
                "symbol": "trx",
                "toAddr": "TBk86hq1e8C1gNX6RDXhk1wLamwzKnotmo",
                "token": "",
                "transactionFee": 1.401,
                "transactionTime": "2024-11-18 09:36:51",
                "transferType": 1,
                "txHash": "d92dbe05593f0c6a5e9dd76ecfb181903ffc8de77c43fa50fd4a6f53042c3371",
                "txKind": 1,
                "value": 1
            },
            "clientId": "dc09781f5671e2bc244ba492b8cfb0af",
            "deviceType": "ANDROID",
            "sn": "9580b55ec4a1d3d3af85077ae0c4c901885b1123e50f830cbd5bfbbe0cb161a3"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_common_transfer_to_multi_receive_is_successful() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "msgId": "779799",
            "appId": "170976fa8a0772b7e2c",
            "bizType": "ACCT_CHANGE",
            "body": {
                "blockHeight": 67090948,
                "chainCode": "tron",
                "fromAddr": "TXBLuUhnfofYZAHZTWxYiDjgoaYLtwPzw3",
                "isMultisig": 0,
                "notes": "时间",
                "queueId": "197131136378474496",
                "status": true,
                "symbol": "trx",
                "toAddr": "TBk86hq1e8C1gNX6RDXhk1wLamwzKnotmo",
                "token": "",
                "transactionFee": 1.401,
                "transactionTime": "2024-11-18 09:36:51",
                "transferType": 0,
                "txHash": "d92dbe05593f0c6a5e9dd76ecfb181903ffc8de77c43fa50fd4a6f53042c3371",
                "txKind": 1,
                "value": 1
            },
            "clientId": "dc09781f5671e2bc244ba492b8cfb0af",
            "deviceType": "ANDROID",
            "sn": "9580b55ec4a1d3d3af85077ae0c4c901885b1123e50f830cbd5bfbbe0cb161a3"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_common_transfer_to_common_token_is_successful() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "msgId": "123123",
            "appId": "120c83f760a33a6c663",
            "bizType": "ACCT_CHANGE",
            "body": {
                "blockHeight": 67174479,
                "chainCode": "tron",
                "fromAddr": "TUDrRQ6zvwXhW3ScTxwGv8nwicLShVVWoF",
                "isMultisig": 0,
                "status": true,
                "symbol": "win",
                "toAddr": "TRbHD77Y6WWDaz9X5esrVKwEVwRM4gTw6N",
                "token": "TLa2f6VPqDgRE67v1736s7bJ8Ray5wYjU7",
                "transactionFee": 0,
                "transactionTime": "2024-11-21 07:14:36",
                "transferType": 1,
                "txHash": "c6a82ee6cd87419116965ceb8060e1318692cf60f90df09c86acf9014a73bec0",
                "txKind": 1,
                "value": 1000
            },
            "clientId": "5931ca06de5ebac2c527ee3e191b6e79",
            "deviceType": "ANDROID",
            "sn": "bf8da3b72e30e0179614e736f34f4a01e379e1383962d7c831922cdabca488b7"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_common_transfer_to_common_fees_is_successful() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "msgId": "321321",
            "appId": "191e35f7e0c29077f08",
            "bizType": "ACCT_CHANGE",
            "body": {
                "blockHeight": 67174479,
                "chainCode": "tron",
                "fromAddr": "TUDrRQ6zvwXhW3ScTxwGv8nwicLShVVWoF",
                "isMultisig": 0,
                "status": true,
                "symbol": "trx",
                "toAddr": "TLa2f6VPqDgRE67v1736s7bJ8Ray5wYjU7",
                "token": "",
                "transactionFee": 5.8414,
                "transactionTime": "2024-11-21 07:14:36",
                "transferType": 1,
                "txHash": "c6a82ee6cd87419116965ceb8060e1318692cf60f90df09c86acf9014a73bec0",
                "txKind": 1,
                "value": 0
            },
            "clientId": "e0c7f98723a4227477968524c580af6c",
            "deviceType": "IOS",
            "sn": "7eeff112cb87de43c54f2fa4b44664a84e469c19c64ab87625857267b5d063c0"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_eth() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "msgId": "321321",
            "appId": "191e35f7e0c29077f08",
            "bizType": "ACCT_CHANGE",
            "body": {
                "blockHeight": 21328166,
                "chainCode": "eth",
                "fromAddr": "0x148805B49819371EEF9A822f7F880b42Cf67834D",
                "isMultisig": 1,
                "status": true,
                "symbol": "ETH",
                "toAddr": "0x8F1E2a99CB688587c02B8b836Ba9Ca39dC60D63B",
                "token": "",
                "transactionFee": 0.000515902158014156,
                "transferType": 0,
                "txHash": "0xba1bbff453e350a47a4d41b395b4c3367ecf68b75bf41c1ee57030e413ec6d5b",
                "txKind": 1,
                "value": 0.000025,
            },
            "clientId": "e0c7f98723a4227477968524c580af6c",
            "deviceType": "IOS",
            "sn": "7eeff112cb87de43c54f2fa4b44664a84e469c19c64ab87625857267b5d063c0"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_eth2() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "msgId": "321321",
            "appId": "18171adc038afd5c4ac",
            "bizType": "ACCT_CHANGE",
            "body": {
                "blockHeight": 21342032,
                "chainCode": "eth",
                "fromAddr": "0x8f1e2a99cb688587c02b8b836ba9ca39dc60d63b",
                "isMultisig": 1,
                "notes": "test",
                "queueId": "203611409211330560",
                "status": true,
                "symbol": "usdt",
                "toAddr": "0x148805b49819371eef9a822f7f880b42cf67834d",
                "token": "0xdac17f958d2ee523a2206206994597c13d831ec7",
                "transactionFee": 0.001107569750308281,
                "transactionTime": "2024-12-06 07:42:23",
                "transferType": 1,
                "txHash": "0xa9143a51d9927cbc010e2b35dfe4c9d4bfc37bd97b02cf7e6e1b2e3f37f91d98",
                "txKind": 1,
                "value": 0.5
            },
            "clientId": "ed8a6aab27108febe9909ff1ae10d31c",
            "deviceType": "IOS",
            "sn": "845edde901f5ea0f7c17d4b2e891679955b0a7b648da1b6ba247dfb1d1ac9902"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_eth3() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "msgId": "321321",
            "appId": "191e35f7e0c290d7abd",
            "bizType": "ACCT_CHANGE",
            "body": {
            "blockHeight": 21342785,
            "chainCode": "eth",
            "fromAddr": "0x8F1E2a99CB688587c02B8b836Ba9Ca39dC60D63B",
            "isMultisig": 1,
            "notes": "0.11223",
            "queueId": "203663273910996992",
             "status": true,
            "symbol": "usdt",
            "toAddr": "0x148805B49819371EEF9A822f7F880b42Cf67834D",
            "token": "0xdac17f958d2ee523a2206206994597c13d831ec7",
            "transactionFee": 0.00135940441821096,
            "transactionTime": "2024-12-06 10:13:47",
            "transferType": 1,
            "txHash": "0xaaa362dfd318f4da95e2d1e71c8c2a2ceabc8fd5df85e7c144843e6fc55f25e0",
            "txKind": 1,
            "value": 0.1112
            },
            "clientId": "7552bd49a9407eb98164c129d11da7e2",
            "deviceType": "IOS",
            "sn": "5bb0eada7cb7290b5d196362e6def48dcb9703e1468c0fb28eb7dd61073875e6"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_eth4() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "msgId": "321321",
            "appId": "191e35f7e0c290d7abd",
            "bizType": "ACCT_CHANGE",
            "body": {
                "blockHeight": 21342785,
                "chainCode": "eth",
                "fromAddr": "0x8F1E2a99CB688587c02B8b836Ba9Ca39dC60D63B",
                "isMultisig": 1,
                "notes": "0.11223",
                "queueId": "203663273910996992",
                "status": true,
                "symbol": "usdt",
                "toAddr": "0x148805B49819371EEF9A822f7F880b42Cf67834D",
                "token": "0xdac17f958d2ee523a2206206994597c13d831ec7",
                "transactionFee": 0.00135940441821096,
                "transactionTime": "2024-12-06 10:13:47",
                "transferType": 1,
                "txHash": "0xaaa362dfd318f4da95e2d1e71c8c2a2ceabc8fd5df85e7c144843e6fc55f25e0",
                "txKind": 1,
                "value": 0.1112
            },
            "clientId": "7552bd49a9407eb98164c129d11da7e2",
            "deviceType": "IOS",
            "sn": "5bb0eada7cb7290b5d196362e6def48dcb9703e1468c0fb28eb7dd61073875e6"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_acct_change_bug() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "appId": "191e35f7e0c290d7abd",
            "bizType": "ACCT_CHANGE",
            "body": {
                "blockHeight": 21336350,
                "chainCode": "eth",
                "fromAddr": "0x148805B49819371EEF9A822f7F880b42Cf67834D",
                "status": false,
                "symbol": "eth",
                "toAddr": "0x8F1E2a99CB688587c02B8b836Ba9Ca39dC60D63B",
                "token": "eth",
                "transactionFee": 0.000596818853647818,
                "transactionTime": "2024-12-05 12:39:11",
                "transferType": 1,
                "txHash": "0xbf49c59702eb886675eec8929b0320fa8ae5e533c9d8f2ab3a845568e4b594ef",
                "txKind": 1,
                "value": 0
            },
            "clientId": "7552bd49a9407eb98164c129d11da7e2",
            "deviceType": "IOS",
            "sn": "5bb0eada7cb7290b5d196362e6def48dcb9703e1468c0fb28eb7dd61073875e6",
            "msgId": "67519ef5a442286e3d1110cd"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }

    #[tokio::test]
    async fn test_acct_exec() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        use rumqttc::v5::mqttbytes::v5::Publish;
        use serde_json::json;

        // 模拟 JSON 数据
        let json_data = json!({
            "appId": "xxx",
             "bizType": "MULTI_SIGN_TRANS_EXECUTE",
            "body": {
                 "withdrawId": "254342466625474560"
            },
            "clientId": "0e567a03c12ca24c5925790ccb348d1f",
            "deviceType": "ANDROID",
            "sn": "guangxiang1",
            "msgId": "680b2ca2db87aa7fc92529e9"
        });

        // 将 JSON 数据转换为 payload
        let payload = serde_json::to_vec(&json_data).expect("Failed to serialize JSON");

        // 创建模拟的 Publish 数据包
        let publish = Publish {
            dup: false,
            qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            retain: false,
            topic: "wallet/order".into(),
            pkid: 0,
            payload: payload.into(),
            properties: Default::default(),
        };

        // 调用 exec_incoming_publish 并断言结果
        let result = exec_incoming_publish(&publish).await;
        assert!(result.is_ok(), "exec_incoming_publish failed: {:?}", result.err());

        Ok(())
    }
}
