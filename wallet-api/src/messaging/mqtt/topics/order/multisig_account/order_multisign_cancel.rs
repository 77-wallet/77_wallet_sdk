use wallet_database::{
    dao::multisig_account::MultisigAccountDaoV1,
    repositories::multisig_account::MultisigAccountRepo,
};

use crate::messaging::notify::{
    FrontendNotifyEvent, event::NotifyEvent, multisig::OrderMultisignCanceledFrontend,
};

// 发起方取消多签账号消息，参与方同步自己多签账号的状态
// biz_type = ORDER_MULTI_SIGN_CANCEL
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrderMultiSignCancel {
    // 多签账户id
    multisig_account_id: String,
}

impl OrderMultiSignCancel {
    pub(crate) fn name(&self) -> String {
        "ORDER_MULTI_SIGN_CANCEL".to_string()
    }
}

impl OrderMultiSignCancel {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let event_name = self.name();
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting to process OrderMultiSignCancel"
        );
        let OrderMultiSignCancel { multisig_account_id } = self;

        let multisig_account = MultisigAccountRepo::found_one_id(multisig_account_id, &pool)
            .await?
            .ok_or(crate::error::service::ServiceError::Business(
                crate::error::business::multisig_account::MultisigAccountError::NotFound.into(),
            ))?;

        // check
        MultisigAccountDaoV1::delete_in_status(multisig_account_id, &*pool)
            .await
            .map_err(|e| crate::error::service::ServiceError::Database(e.into()))?;

        let data = NotifyEvent::OrderMultisignCanceled(OrderMultisignCanceledFrontend {
            multisig_account_id: multisig_account.id,
            multisig_account_address: multisig_account.address,
            address_type: multisig_account.address_type,
        });
        FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{messaging::mqtt::topics::OrderMultiSignCancel, test::env::get_manager};

    #[tokio::test]
    async fn test_() {
        wallet_utils::init_test_log();
        let (_, _) = get_manager().await.unwrap();

        let raw = r#"{"multisigAccountId": "256890128948137984"}"#;
        let res = serde_json::from_str::<OrderMultiSignCancel>(&raw).unwrap();

        let _c = res.exec("x").await.unwrap();
    }
}
