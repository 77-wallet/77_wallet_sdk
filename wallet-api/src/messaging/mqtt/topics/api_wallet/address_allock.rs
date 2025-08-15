use crate::{
    domain::api_wallet::account::ApiAccountDomain,
    messaging::notify::{event::NotifyEvent, FrontendNotifyEvent},
};

// biz_type = ADDRESS_ALLOCK
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddressAllockMsg {
    /// uid
    pub r#type: String,
    pub chain: String,
    pub index: i32,
    pub uid: String,
}

// 地址池扩容
impl AddressAllockMsg {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        // ApiAccountDomain::address_used(&self.chain_code, self.index, &self.uid, None).await?;

        // let data = NotifyEvent::AddressUse(self.to_owned());
        // FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}
