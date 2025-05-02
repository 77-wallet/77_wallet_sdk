use crate::{
    domain::multisig::MultisigDomain, messaging::mqtt::topics::OrderMultiSignAcceptCompleteMsg,
};
use wallet_database::entities::multisig_account::MultisigAccountStatus;

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

        // let account =
        //     MultisigDomain::check_multisig_account_exists(&self.multisig_account_id).await?;

        // let Some(_account) = account else {
        //     tracing::warn!(
        //         "[OrderAllConfirmed] faild account not found {}",
        //         self.multisig_account_id
        //     );
        //     return Ok(());
        // };

        // if _account.status == MultisigAccountStatus::Pending.to_i8() {
        //     OrderMultiSignAcceptCompleteMsg::all_members_confirmed(
        //         &self.members,
        //         &self.multisig_account_id,
        //         1,
        //     )
        //     .await?;
        // }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{messaging::mqtt::topics::OrderAllConfirmed, test::env::get_manager};

    #[tokio::test]
    async fn test_() {
        wallet_utils::init_test_log();
        let (_, _) = get_manager().await.unwrap();

        let raw = r#"{"noticeTitle":"多签账户等待您部署","noticeContent":"多签账户等待您部署","multisigAccountId":"256946775234056192","members":[{"name":"发起人","address":"TSwxhEpTdpGRM4MUqd6Gjojh8jdrUUPsRC","pubkey":"044784B00BF046B69D4D77C18AA9805219E41C117D91E249076866EAB8FE25B1BCC03D6FD0C1D68B09071A719EE95B2CD8A7E8FACD9B7BD85D8B17EBE6B92C1622","status":1,"uid":"dd8e4970ef357c64e325a8a1afb76ba8884f276b0f4a807926e4337bf5fa62ca"},{"name":"2","address":"TV7NLrNDhmB7r7KWuCxxFJ1ipUrzUpzAXW","pubkey":"","status":0,"uid":"52e024f827487016441466927be7eccb23347786c0b0a91834301ddc9cf15434"},{"name":"3","address":"TAY1AH4wrxjB4g7zvZDzJuDxLYMGn9duZX","pubkey":"","status":0,"uid":"968e6d5bb60ddd784fa298d2bd5034ecb7d0b9b09a15699d2b2884554f507e58"}]}"#;
        let res = serde_json::from_str::<OrderAllConfirmed>(&raw).unwrap();

        let _c = res.exec("x").await.unwrap();
    }
}
