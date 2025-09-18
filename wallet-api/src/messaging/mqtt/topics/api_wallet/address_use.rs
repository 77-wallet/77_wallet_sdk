use crate::{
    domain::api_wallet::account::ApiAccountDomain,
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
};

// biz_type = ADDRESS_USE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddressUseMsg {
    /// uid
    pub uid: String,
    #[serde(rename = "chain")]
    pub chain_code: String,
    pub index: i32,
    pub address: String,
}

// 地址使用
impl AddressUseMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        ApiAccountDomain::address_used(&self.chain_code, self.index, &self.uid).await?;

        let data = NotifyEvent::AddressUse(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}
