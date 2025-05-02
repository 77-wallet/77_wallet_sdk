use crate::{
    domain::multisig::MultisigDomain, messaging::mqtt::topics::OrderMultiSignAcceptCompleteMsg,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrderAllConfirmed {
    /// 多签账户id
    multisig_account_id: String,
    /// 所有参与方地址
    members: Vec<wallet_transport_backend::ConfirmedAddress>,
}

impl OrderAllConfirmed {
    pub(crate) fn name(&self) -> String {
        "ORDER_MULTI_SIGN_ALL_MEMBER_ACCEPTED".to_string()
    }
}

impl OrderAllConfirmed {
    pub(crate) async fn exec(self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let event_name = self.name();
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting to process OrderAllConfirmed"
        );

        let account =
            MultisigDomain::check_multisig_account_exists(&self.multisig_account_id).await?;

        let Some(_account) = account else {
            tracing::warn!(
                "[OrderAllConfirmed] faild account not found {}",
                self.multisig_account_id
            );
            return Ok(());
        };

        OrderMultiSignAcceptCompleteMsg::all_members_confirmed(
            &self.members,
            &self.multisig_account_id,
            1,
        )
        .await?;

        Ok(())
    }
}
