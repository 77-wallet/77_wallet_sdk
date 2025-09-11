use wallet_transport_backend::request::api_wallet::address::{AddressParam, ExpandAddressReq};

use crate::domain::api_wallet::wallet::ApiWalletDomain;

// biz_type = ADDRESS_ALLOCK
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddressAllockMsg {
    /// 扩容类型： CHA_ALL / CHA_INDEX
    #[serde(rename = "type")]
    pub typ: AddressAllockType,
    #[serde(rename = "chain")]
    pub chain_code: String,
    pub index: Option<i32>,
    pub uid: String,
    pub serial_no: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AddressAllockType {
    ChaBatch,
    ChaIndex,
}

// 地址池扩容
impl AddressAllockMsg {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        // let data = NotifyEvent::AddressUse(self.to_owned());
        // FrontendNotifyEvent::new(data).send().await?;

        ApiWalletDomain::expand_address(&self.typ, self.index, &self.uid, &self.chain_code).await?;

        Ok(())
    }
}
