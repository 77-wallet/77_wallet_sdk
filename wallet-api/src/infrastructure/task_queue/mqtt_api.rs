use crate::{
    error::service::ServiceError,
    infrastructure::task_queue::task::{task_type::TaskType, TaskTrait},
    messaging::mqtt::topics::api_wallet::{
        cmd::{
            address_allock::AwmCmdAddrExpandMsg, unbind_uid::AwmCmdUidUnbindMsg,
            wallet_activation::AwmCmdActiveMsg,
            fee_res::AwmCmdFeeResMsg,
        },
        trans::AwmOrderTransMsg,
        trans_result::AwmOrderTransResMsg,
    },
};
use wallet_database::entities::task_queue::{KnownTaskName, TaskName};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
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

    fn get_body(&self) -> Result<Option<String>, ServiceError> {
        Ok(Some(wallet_utils::serde_func::serde_to_string(self)?))
    }

    async fn execute(&self, id: &str) -> Result<(), ServiceError> {
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
                let data: AwmCmdFeeResMsg =
                    wallet_utils::serde_func::serde_from_value(self.data.clone())?;
                data.exec(id).await?
            }
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
