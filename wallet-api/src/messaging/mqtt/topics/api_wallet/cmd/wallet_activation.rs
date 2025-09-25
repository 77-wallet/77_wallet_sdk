use wallet_transport_backend::response_vo::api_wallet::wallet::ActiveStatus;

use crate::messaging::notify::{FrontendNotifyEvent, event::NotifyEvent};

// biz_type = WALLET_ACTIVATION
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmCmdActiveMsg {
    #[serde(rename = "chain")]
    pub chain_code: String,
    pub uid: String,
    /// 激活状态: 0 未激活 /1已激活
    pub active: ActiveStatus,
}

impl AwmCmdActiveMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let data = NotifyEvent::AwmCmdActive(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;

        // ApiWalletDomain::expand_address(&self.typ, self.index, &self.uid, &self.chain_code).await?;
        tracing::info!("WalletActivationMsg: {:?}", self);
        Ok(())
    }
}
