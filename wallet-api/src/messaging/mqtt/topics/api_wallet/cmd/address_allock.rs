use crate::{
    domain::api_wallet::wallet::ApiWalletDomain,
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
};

// biz_type = ADDRESS_ALLOCK
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmCmdAddrExpandMsg {
    /// 扩容类型： CHA_ALL / CHA_INDEX
    #[serde(rename = "type")]
    pub typ: AddressAllockType,
    #[serde(rename = "chain")]
    pub chain_code: String,
    pub index: Option<i32>,
    pub uid: String,
    /// 扩容编号  
    pub serial_no: String,
    /// 扩容数量（可空，CHA_BATCH 类型时有效）
    #[serde(deserialize_with = "wallet_utils::serde_func::string_to_u32")]
    pub number: u32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AddressAllockType {
    ChaBatch,
    ChaIndex,
}

// 地址池扩容
impl AwmCmdAddrExpandMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        ApiWalletDomain::expand_address(
            &self.typ,
            self.index,
            &self.uid,
            &self.chain_code,
            self.number,
            &self.serial_no,
        )
        .await?;

        let data = NotifyEvent::AwmCmdAddrExpand(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {

    use crate::{infrastructure::task_queue::ApiMqttStruct, messaging::mqtt::Message};

    #[test]
    fn deserialize() {
        let data = "{\"bizType\":\"AWM_CMD_ADDR_EXPAND\",\"body\":{\"data\":{\"type\":\"CHA_BATCH\",\"chain\":\"tron\",\"uid\":\"eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c\",\"serialNo\":\"tron_eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c\",\"number\":\"10\"},\"eventNo\":\"1971130334984785920\",\"eventType\":\"3\",\"time\":1758789068},\"clientId\":\"df1b2982f3240f55fa8769e38e747010\",\"deviceType\":\"ANDROID\",\"sn\":\"5a748300e76e023cea05523c103763a7976bdfb085c24f9713646ae2faa5949d\",\"msgId\":\"68d4fdcdab00e34b73ef17a0\"}";

        let msg: Message = serde_json::from_str(data).unwrap();
        println!("{:#?}", msg);

        let msg: ApiMqttStruct = serde_json::from_value(msg.body).unwrap();
        println!("result: {:#?}", msg);
    }
}
