use crate::{
    domain::api_wallet::account::ApiAccountDomain,
    messaging::notify::{event::NotifyEvent, FrontendNotifyEvent},
};

// biz_type = RECHARGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawMsg {
    /// uid
    pub uid: String,
    pub chain_code: String,
    pub index: i32,
    pub address: String,
}

// 提现
impl WithdrawMsg {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        // 验证金额是否需要输入密码

        // 生成订单

        // 上链

        // ApiAccountDomain::address_used(&self.chain_code, self.index, &self.uid, None).await?;

        // let data = NotifyEvent::AddressUse(self.to_owned());
        // FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}
