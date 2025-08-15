use crate::{
    domain::api_wallet::account::ApiAccountDomain,
    messaging::notify::{event::NotifyEvent, FrontendNotifyEvent},
};

// biz_type = RECHARGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CollectionMsg {
    /// uid
    pub uid: String,
    pub chain_code: String,
    pub index: i32,
    pub address: String,
}

// 归集
impl CollectionMsg {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        // 校验手续费

        // 够 -》执行归集 -》 通知后端 -》 归集上链

        // 不够 -》出款地址余额 -》不够 -》 通知前端 余额不足

        // 够 -》 转账手续费 -》 通知后端手续费转账

        // 生成订单

        // 上链

        // ApiAccountDomain::address_used(&self.chain_code, self.index, &self.uid, None).await?;

        // let data = NotifyEvent::AddressUse(self.to_owned());
        // FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}
