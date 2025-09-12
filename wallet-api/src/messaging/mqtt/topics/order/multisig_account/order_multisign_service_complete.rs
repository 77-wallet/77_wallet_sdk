use wallet_database::{
    DbPool,
    dao::multisig_account::MultisigAccountDaoV1,
    entities::multisig_account::{
        MultiAccountOwner, MultisigAccountEntity, MultisigAccountPayStatus, MultisigAccountStatus,
    },
};

use crate::{
    domain::multisig::MultisigDomain,
    messaging::notify::{
        FrontendNotifyEvent, event::NotifyEvent, multisig::OrderMultiSignServiceCompleteFrontend,
        other::ErrFront,
    },
};

// 部署完成后，服务费hash以及部署hash的同步。
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrderMultiSignServiceComplete {
    // 多签账户id
    multisig_account_id: String,
    // 多签账号结果 true 多签账号或服务费执行完成  false 失败
    status: bool,
    // 1: 多签账号手续费 2: 服务费
    r#type: u8,
}

impl OrderMultiSignServiceComplete {
    pub(crate) fn name(&self) -> String {
        "ORDER_MULTI_SIGN_SERVICE_COMPLETE".to_string()
    }
}

impl OrderMultiSignServiceComplete {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let event_name = self.name();
        let pool = crate::context::Context::get_global_sqlite_pool()?;
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting to process OrderMultiSignServiceComplete"
        );

        let OrderMultiSignServiceComplete { multisig_account_id, status, r#type } = self;

        let account = Self::get_account_or_recover(multisig_account_id, &pool, &event_name).await?;

        let multi_account_id = account.id;

        // 更新多签账户手续费状态
        let (status, pay_status) = Self::get_status(*r#type, *status);
        MultisigAccountDaoV1::update_status(&multi_account_id, status, pay_status, pool.as_ref())
            .await
            .map_err(crate::ServiceError::Database)?;

        // 不是发起方更重新上报状态
        if account.owner != MultiAccountOwner::Participant.to_i8() {
            let _r = MultisigDomain::update_raw_data(&multi_account_id, pool).await;
        }

        let data =
            NotifyEvent::OrderMultiSignServiceComplete(OrderMultiSignServiceCompleteFrontend {
                multisign_address: account.address,
                status: self.status,
                r#type: *r#type,
            });
        FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }

    async fn get_account_or_recover(
        multisig_account_id: &str,
        pool: &DbPool,
        event_name: &str,
    ) -> Result<MultisigAccountEntity, crate::ServiceError> {
        // 第一次查询
        let mut account =
            MultisigAccountDaoV1::find_by_id(multisig_account_id, pool.as_ref()).await?;
        if account.is_none() {
            MultisigDomain::recover_multisig_account_by_id(multisig_account_id).await?;

            account = MultisigAccountDaoV1::find_by_id(multisig_account_id, pool.as_ref()).await?;
        }
        // 判断最终是否查询到数据
        if let Some(account) = account {
            Ok(account)
        } else {
            tracing::error!(
                event_name = %event_name,
                multisig_account_id = %multisig_account_id,
                "Multisig account not found"
            );
            let err = crate::ServiceError::Business(crate::MultisigAccountError::NotFound.into());
            let data = NotifyEvent::Err(ErrFront {
                event: event_name.to_string(),
                message: err.to_string(),
            });
            FrontendNotifyEvent::new(data).send().await?;
            Err(err)
        }
    }

    fn get_status(types: u8, status: bool) -> (Option<i8>, Option<i8>) {
        if types == 1 {
            if status {
                (Some(MultisigAccountStatus::OnChain.to_i8()), None)
            } else {
                (Some(MultisigAccountStatus::OnChainFail.to_i8()), None)
            }
        } else if status {
            (None, Some(MultisigAccountPayStatus::Paid.to_i8()))
        } else {
            (None, Some(MultisigAccountPayStatus::PaidFail.to_i8()))
        }
    }
}

#[cfg(test)]
mod test {
    use crate::messaging::mqtt::topics::OrderMultiSignServiceComplete;

    #[test]
    fn test_() {
        let raw = r#"
        {
            "hash": "dc91c064567fa7276d5888c7973e44f74c64a9b89b9517346ca677f550929cf1",
            "status": true,
            "type": 1,
            "orderId": "66b328e9eafe8b248415bbb3"
        }
        "#;
        let res = serde_json::from_str::<OrderMultiSignServiceComplete>(&raw);
        println!("res: {res:?}");
    }
}
