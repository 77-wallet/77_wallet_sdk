use crate::{
    domain::api_wallet::account::ApiAccountDomain,
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
};

// biz_type = ADDRESS_USE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddressUseItem {
    /// uid
    pub uid: String,
    #[serde(rename = "chain")]
    pub chain_code: String,
    pub index: i32,
    pub client_id: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct AddressUseMsg(Vec<AddressUseItem>);

impl AddressUseMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        for item in self.0.iter() {
            ApiAccountDomain::address_used(&item.chain_code, item.index, &item.uid).await?;
        }

        let data = NotifyEvent::AddressUse(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}
