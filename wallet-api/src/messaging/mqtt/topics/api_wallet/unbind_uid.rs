use crate::{
    domain::api_wallet::wallet::ApiWalletDomain,
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
};

// biz_type = UNBIND_UID
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UnbindUidMsg {
    /// uid
    pub uid: String,
}

impl UnbindUidMsg {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let Self { uid } = self;
        ApiWalletDomain::unbind_uid(uid).await?;
        let data = NotifyEvent::UnbindUid(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}
