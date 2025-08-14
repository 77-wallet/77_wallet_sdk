use crate::{
    domain::api_wallet::account::ApiAccountDomain,
    messaging::notify::{event::NotifyEvent, FrontendNotifyEvent},
};

// biz_type = UNBIND_UID
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddressUseMsg {
    /// uid
    pub uid: String,
    pub chain_code: String,
    pub index: i32,
    pub address: String,
}

// 按下标递增
impl AddressUseMsg {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        ApiAccountDomain::address_used(&self.chain_code, self.index, &self.uid, None).await?;

        let data = NotifyEvent::AddressUse(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}
