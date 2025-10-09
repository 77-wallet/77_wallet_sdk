use wallet_database::entities::task_queue::{KnownTaskName, TaskName};

use crate::{
    infrastructure::task_queue::task::{TaskTrait, task_type::TaskType},
    messaging::mqtt::topics::{
        self,
        api_wallet::{
            cmd::AwmCmdMsg, trans::AwmOrderTransMsg, trans_result::AwmOrderTransResMsg, cmd::address_allock::AwmCmdAddrExpandMsg,
            cmd::unbind_uid::AwmCmdUidUnbindMsg, cmd::wallet_activation::AwmCmdActiveMsg,
        },
    },
};
use crate::messaging::mqtt::topics::api_wallet::cmd::address_use::AddressUseMsg;
use crate::messaging::mqtt::topics::api_wallet::trans_fee_result::AwmOrderTransFeeResMsg;

#[async_trait::async_trait]
impl TaskTrait for MqttTask {
    fn get_name(&self) -> TaskName {
        match self {
            MqttTask::OrderMultiSignAccept(_) => {
                TaskName::Known(KnownTaskName::OrderMultiSignAccept)
            }
            MqttTask::OrderMultiSignAcceptCompleteMsg(_) => {
                TaskName::Known(KnownTaskName::OrderMultiSignAcceptCompleteMsg)
            }
            MqttTask::OrderMultiSignServiceComplete(_) => {
                TaskName::Known(KnownTaskName::OrderMultiSignServiceComplete)
            }
            MqttTask::OrderMultiSignCreated(_) => {
                TaskName::Known(KnownTaskName::OrderMultiSignCreated)
            }
            MqttTask::OrderMultiSignCancel(_) => {
                TaskName::Known(KnownTaskName::OrderMultiSignCancel)
            }
            MqttTask::MultiSignTransAccept(_) => {
                TaskName::Known(KnownTaskName::MultiSignTransAccept)
            }
            MqttTask::MultiSignTransCancel(_) => {
                TaskName::Known(KnownTaskName::MultiSignTransCancel)
            }
            MqttTask::MultiSignTransAcceptCompleteMsg(_) => {
                TaskName::Known(KnownTaskName::MultiSignTransAcceptCompleteMsg)
            }
            MqttTask::AcctChange(_) => TaskName::Known(KnownTaskName::AcctChange),
            MqttTask::BulletinMsg(_) => TaskName::Known(KnownTaskName::BulletinMsg),

            MqttTask::PermissionAccept(_) => TaskName::Known(KnownTaskName::PermissionAccept),
            MqttTask::MultiSignTransExecute(_) => {
                TaskName::Known(KnownTaskName::MultiSignTransExecute)
            }
            MqttTask::CleanPermission(_) => TaskName::Known(KnownTaskName::CleanPermission),
            MqttTask::OrderAllConfirmed(_) => TaskName::Known(KnownTaskName::OrderAllConfirmed),
            // api wallet
            MqttTask::ApiMqttStruct(api_mqtt_struct) => api_mqtt_struct.get_name(),
        }
    }
    fn get_type(&self) -> TaskType {
        TaskType::Mqtt
    }
    fn get_body(&self) -> Result<Option<String>, crate::error::service::ServiceError> {
        let res = match self {
            MqttTask::OrderMultiSignAccept(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::OrderMultiSignAcceptCompleteMsg(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::OrderMultiSignServiceComplete(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::OrderMultiSignCancel(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::MultiSignTransAccept(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::MultiSignTransCancel(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::MultiSignTransAcceptCompleteMsg(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::OrderMultiSignCreated(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::AcctChange(req) => Some(wallet_utils::serde_func::serde_to_string(req)?),
            MqttTask::BulletinMsg(req) => Some(wallet_utils::serde_func::serde_to_string(req)?),
            MqttTask::PermissionAccept(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::MultiSignTransExecute(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::OrderAllConfirmed(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::CleanPermission(req) => Some(wallet_utils::serde_func::serde_to_string(req)?),
            MqttTask::ApiMqttStruct(api_mqtt_struct) => api_mqtt_struct.get_body()?,
        };
        Ok(res)
    }

    async fn execute(&self, id: &str) -> Result<(), crate::error::service::ServiceError> {
        match self {
            MqttTask::OrderMultiSignAccept(data) => data.exec(id).await?,
            MqttTask::OrderMultiSignAcceptCompleteMsg(data) => data.exec(id).await?,
            MqttTask::OrderMultiSignServiceComplete(data) => data.exec(id).await?,
            MqttTask::OrderMultiSignCreated(data) => data.exec(id).await?,
            MqttTask::OrderMultiSignCancel(data) => data.exec(id).await?,
            MqttTask::MultiSignTransAccept(data) => data.exec(id).await?,
            MqttTask::MultiSignTransCancel(data) => data.exec(id).await?,
            MqttTask::MultiSignTransAcceptCompleteMsg(data) => data.exec(id).await?,
            MqttTask::AcctChange(data) => data.exec(id).await?,
            MqttTask::BulletinMsg(data) => data.exec(id).await?,
            MqttTask::PermissionAccept(data) => data.exec(id).await?,
            MqttTask::MultiSignTransExecute(data) => data.exec(id).await?,
            MqttTask::CleanPermission(data) => data.exec(id).await?,
            MqttTask::OrderAllConfirmed(data) => data.exec(id).await?,
            MqttTask::ApiMqttStruct(api_mqtt_struct) => api_mqtt_struct.execute(id).await?,
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(crate) enum MqttTask {
    OrderMultiSignAccept(topics::OrderMultiSignAccept),
    OrderMultiSignAcceptCompleteMsg(topics::OrderMultiSignAcceptCompleteMsg),
    OrderMultiSignServiceComplete(topics::OrderMultiSignServiceComplete),
    OrderMultiSignCreated(topics::OrderMultiSignCreated),
    OrderAllConfirmed(topics::OrderAllConfirmed),
    OrderMultiSignCancel(topics::OrderMultiSignCancel),
    MultiSignTransAccept(topics::MultiSignTransAccept),
    MultiSignTransCancel(topics::MultiSignTransCancel),
    MultiSignTransAcceptCompleteMsg(topics::MultiSignTransAcceptCompleteMsg),
    MultiSignTransExecute(topics::MultiSignTransExecute),
    AcctChange(topics::AcctChange),
    BulletinMsg(topics::BulletinMsg),
    PermissionAccept(topics::PermissionAccept),
    CleanPermission(topics::CleanPermission),

    ApiMqttStruct(ApiMqttStruct),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApiMqttStruct {
    pub(crate) event_no: String,
    /// 1交易事件 / 2 交易最终结果 / 3 地址扩容 / 4平台解绑/ 5 激活钱包 / 6 交易手续费结果 /
    pub(crate) event_type: EventType,
    pub(crate) data: serde_json::Value,
    pub(crate) time: u64,
    pub(crate) sign: Option<String>,
    pub(crate) secret: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(crate) enum EventType {
    #[serde(rename = "1")]
    AwmOrderTrans,
    #[serde(rename = "2")]
    AwmOrderTransRes,
    #[serde(rename = "3")]
    AwmCmdAddrExpand,
    #[serde(rename = "4")]
    AwmCmdUidUnbind,
    #[serde(rename = "5")]
    AwmCmdActive,
    #[serde(rename = "6")]
    AwmCmdFeeRes,
    // AddressUse,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub(crate) enum ApiMqttData {
    /// 推送交易消息
    AwmOrderTrans(AwmOrderTransMsg),
    /// 交易结果通知
    AwmOrderTransRes(AwmOrderTransResMsg),
    /// 命令
    AwmCmd(AwmCmdMsg),
    AddressUse(AddressUseMsg),
}

#[async_trait::async_trait]
impl TaskTrait for ApiMqttStruct {
    fn get_name(&self) -> TaskName {
        match self.event_type {
            EventType::AwmOrderTrans => TaskName::Known(KnownTaskName::AwmOrderTrans),
            EventType::AwmOrderTransRes => TaskName::Known(KnownTaskName::AwmOrderTransRes),
            EventType::AwmCmdAddrExpand => TaskName::Known(KnownTaskName::AwmCmdAddrExpand),
            EventType::AwmCmdUidUnbind => TaskName::Known(KnownTaskName::AwmCmdUidUnbind),
            EventType::AwmCmdFeeRes => TaskName::Known(KnownTaskName::AwmCmdFeeRes),
            EventType::AwmCmdActive => TaskName::Known(KnownTaskName::AwmCmdActive),
        }
    }

    fn get_type(&self) -> TaskType {
        TaskType::Mqtt
    }

    fn get_body(&self) -> Result<Option<String>, crate::error::service::ServiceError> {
        // let res = match &self.data {
        //     ApiMqttData::AwmOrderTrans(trans_msg) => {
        //         Some(wallet_utils::serde_func::serde_to_string(trans_msg)?)
        //     }
        //     ApiMqttData::AwmOrderTransRes(trans_result_msg) => {
        //         Some(wallet_utils::serde_func::serde_to_string(trans_result_msg)?)
        //     }
        //     ApiMqttData::AwmCmd(msg) => Some(wallet_utils::serde_func::serde_to_string(msg)?),

        //     ApiMqttData::AddressUse(address_use_msg) => {
        //         Some(wallet_utils::serde_func::serde_to_string(address_use_msg)?)
        //     }
        // };

        // Ok(res)

        Ok(Some(wallet_utils::serde_func::serde_to_string(self)?))
    }

    async fn execute(&self, id: &str) -> Result<(), crate::error::service::ServiceError> {
        match &self.event_type {
            EventType::AwmOrderTrans => {
                let data: AwmOrderTransMsg =
                    wallet_utils::serde_func::serde_from_value(self.data.clone())?;
                data.exec(id).await?
            }
            EventType::AwmOrderTransRes => {
                let data: AwmOrderTransResMsg =
                    wallet_utils::serde_func::serde_from_value(self.data.clone())?;
                data.exec(id).await?
            }
            EventType::AwmCmdAddrExpand => {
                let data: AwmCmdAddrExpandMsg =
                    wallet_utils::serde_func::serde_from_value(self.data.clone())?;
                data.exec(id).await?
            }
            EventType::AwmCmdUidUnbind => {
                let data: AwmCmdUidUnbindMsg =
                    wallet_utils::serde_func::serde_from_value(self.data.clone())?;
                data.exec(id).await?
            }
            EventType::AwmCmdFeeRes => {
                let data: AwmOrderTransFeeResMsg =
                    wallet_utils::serde_func::serde_from_value(self.data.clone())?;
                data.exec(id).await?
            },
            EventType::AwmCmdActive => {
                let data: AwmCmdActiveMsg =
                    wallet_utils::serde_func::serde_from_value(self.data.clone())?;
                data.exec(id).await?
            }
        }

        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
