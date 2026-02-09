use wallet_database::repositories::api_wallet::{
    address_query_state::AddressQueryStateRepo, wallet::ApiWalletRepo,
};
use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;

use crate::{
    domain::api_wallet::wallet::ApiWalletDomain,
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
};

// biz_type = AWM_CMD_DEV_CHANGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmCmdDevChangeMsg {
    pub new_sn: String,
    pub uid: String,
}

// 更换设备
impl AwmCmdDevChangeMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let recharge_wallet = ApiWalletRepo::find_by_uid(&pool, &self.uid).await?;

        if let Some(recharge_wallet) = recharge_wallet {
            ApiWalletDomain::db_save_sn_data(
                &recharge_wallet.address,
                recharge_wallet.binding_address.as_deref(),
                &self.new_sn,
            )
            .await?;
            AddressQueryStateRepo::delete_by_uid(&pool, &self.uid).await?;
            if let Some(binding_address) = recharge_wallet.binding_address.as_deref() {
                let withdraw_wallet =
                    ApiWalletRepo::find_by_address(&pool, binding_address).await?;
                if let Some(withdraw_wallet) = withdraw_wallet {
                    AddressQueryStateRepo::delete_by_uid(&pool, &withdraw_wallet.uid).await?;
                }
            }
        }

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(_msg_id);
        backend.msg_ack(msg_ack_req).await?;
        let data = NotifyEvent::AwmCmdDevChange(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {

    use crate::{infrastructure::task_queue::mqtt_api::ApiMqttStruct, messaging::mqtt::Message};

    #[test]
    fn deserialize() {
        let data = "{\"bizType\":\"AWM_CMD_ADDR_EXPAND\",\"body\":{\"data\":{\"type\":\"CHA_BATCH\",\"chain\":\"tron\",\"uid\":\"eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c\",\"serialNo\":\"tron_eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c\",\"number\":\"10\"},\"eventNo\":\"1971130334984785920\",\"eventType\":\"3\",\"time\":1758789068},\"clientId\":\"df1b2982f3240f55fa8769e38e747010\",\"deviceType\":\"ANDROID\",\"sn\":\"5a748300e76e023cea05523c103763a7976bdfb085c24f9713646ae2faa5949d\",\"msgId\":\"68d4fdcdab00e34b73ef17a0\"}";

        let msg: Message = serde_json::from_str(data).unwrap();
        println!("{:#?}", msg);

        let msg: ApiMqttStruct = serde_json::from_value(msg.body).unwrap();
        println!("result: {:#?}", msg);
    }
}
