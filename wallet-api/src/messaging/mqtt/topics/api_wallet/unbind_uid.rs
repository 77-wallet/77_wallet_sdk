use crate::{
    domain::api_wallet::wallet::ApiWalletDomain,
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
};

// biz_type = UNBIND_UID
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmCmdUidUnbindMsg {
    /// uid
    pub uid: String,
}

impl AwmCmdUidUnbindMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let Self { uid } = self;
        ApiWalletDomain::unbind_uid(uid).await?;
        let data = NotifyEvent::AwmCmdUidUnbind(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}
