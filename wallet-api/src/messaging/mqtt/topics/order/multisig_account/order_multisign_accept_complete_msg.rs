use wallet_database::{
    dao::{multisig_account::MultisigAccountDaoV1, multisig_member::MultisigMemberDaoV1},
    entities::multisig_account::MultisigAccountStatus,
};

use crate::{
    domain::multisig::MultisigDomain,
    messaging::notify::{
        FrontendNotifyEvent, event::NotifyEvent, multisig::OrderMultiSignAcceptCompleteMsgFrontend,
        other::ErrFront,
    },
};
// 参与放同意参与多签
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrderMultiSignAcceptCompleteMsg {
    /// 参与状态(同意1,不同意0)
    status: i32,
    /// 多签账户id
    multisig_account_id: String,
    /// 本次同意的参与方地址
    accept_address_list: Vec<String>,
    /// 所有参与方地址
    address_list: Vec<wallet_transport_backend::ConfirmedAddress>,
    accept_status: bool,
}
impl OrderMultiSignAcceptCompleteMsg {
    pub(crate) fn name(&self) -> String {
        "ORDER_MULTI_SIGN_ACCEPT_COMPLETE_MSG".to_string()
    }
}

// 参与方同意后、同步数据给其他的成员同步对应的状态数据(多签账号数据状态流转)
impl OrderMultiSignAcceptCompleteMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let event_name = self.name();
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting to process OrderMultiSignAcceptCompleteMsg"
        );

        let OrderMultiSignAcceptCompleteMsg {
            status,
            multisig_account_id,
            accept_address_list: _,
            address_list,
            accept_status,
        } = &self;

        let account = MultisigDomain::check_multisig_account_exists(multisig_account_id).await?;

        let Some(account) = account else {
            tracing::error!(event_name = %event_name, multisig_account_id = %multisig_account_id, "multisig account not found");
            let err = crate::error::service::ServiceError::Business(
                crate::error::business::multisig_account::MultisigAccountError::NotFound.into(),
            );

            let data = NotifyEvent::Err(ErrFront { event: event_name, message: err.to_string() });
            FrontendNotifyEvent::new(data).send().await?;
            return Err(err);
        };

        Self::all_members_confirmed(address_list, &account.id, account.status).await?;
        tracing::info!(
            event_name = %event_name,
            multisig_account_id = %account.id,
            "All members confirmed for account"
        );
        Self::send_to_frontend(
            *status as i8,
            &account.address,
            address_list.to_vec(),
            *accept_status,
        )
        .await?;

        Ok(())
    }

    pub async fn all_members_confirmed(
        address_list: &[wallet_transport_backend::ConfirmedAddress],
        multi_account_id: &str,
        status: i8,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        for address in address_list.iter() {
            MultisigMemberDaoV1::sync_confirmed_and_pubkey_status(
                multi_account_id,
                &address.address,
                &address.pubkey,
                address.status,
                &address.uid,
                pool.as_ref(),
            )
            .await
            .map_err(|e| crate::error::service::ServiceError::Database(e.into()))?;

            let member = MultisigMemberDaoV1::find_records_by_id(multi_account_id, pool.as_ref())
                .await
                .map_err(|e| crate::error::service::ServiceError::Database(e.into()))?;

            tracing::info!(
                multi_account_id = %multi_account_id,
                address = %address.address,
                "Successfully synced confirmed status for member"
            );
            let mut flag = true;
            for item in member.0.iter() {
                if item.confirmed != 1 {
                    flag = false;
                    break;
                }
            }
            if flag && status == MultisigAccountStatus::Pending.to_i8() {
                // 所有owner都确认过，将多签账户的状态设待部署
                MultisigAccountDaoV1::sync_status(
                    multi_account_id,
                    MultisigAccountStatus::Confirmed,
                    pool.as_ref(),
                )
                .await
                .map_err(crate::error::service::ServiceError::Database)?;
            }
        }
        Ok(())
    }

    async fn send_to_frontend(
        status: i8,
        address: &str,
        address_list: Vec<wallet_transport_backend::ConfirmedAddress>,
        accept_status: bool,
    ) -> Result<(), crate::error::service::ServiceError> {
        let data =
            NotifyEvent::OrderMultiSignAcceptCompleteMsg(OrderMultiSignAcceptCompleteMsgFrontend {
                status,
                multisign_address: address.to_string(),
                address_list,
                accept_status,
            });
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::messaging::mqtt::Message;

    #[test]
    fn test_() {
        let raw = r#"
        {
            "bizType": "ORDER_MULTI_SIGN_ACCEPT_COMPLETE_MSG",
            "body": {
                "orderId": "66b328e9eafe8b248415bbb3",
                "status": 1,
                "acceptStatus": false,
                "confirmList": [
                    {
                        "acceptAddress": "THx9ao6pdLUFoS3CSc98pwj1HCrmGHoVUB",
                        "acceptStatus": true,
                        "createTime": "2024-08-07 15:57:37.268",
                        "id": "66b328f1eafe8b248415bbb5",
                        "messageContent": "{\"id\":\"159780934751752192\",\"orderId\":\"66b328e9eafe8b248415bbb3\",\"name\":\"Multisig-tron-1\",\"initiatorAddr\":\"TJk5nUGoaMFmcrmSubFD11w6DVf5uX5yi6\",\"address\":\"TJk5nUGoaMFmcrmSubFD11w6DVf5uX5yi6\",\"chainCode\":\"tron\",\"threshold\":2,\"memeber\":[{\"name\":\"alice\",\"address\":\"THx9ao6pdLUFoS3CSc98pwj1HCrmGHoVUB\"},{\"name\":\"bob\",\"address\":\"TByQCQiBUtbLQNh6r1ZPNwBJC1jLgZjkuk\"},{\"name\":\"bo\",\"address\":\"TJk5nUGoaMFmcrmSubFD11w6DVf5uX5yi6\"}]}",
                        "orderId": "66b328e9eafe8b248415bbb3",
                        "sendMessage": true,
                        "status": 1,
                        "updateTime": "2024-08-07 15:57:37.268"
                    }
                ],
                "acceptAddressList": [
                    "THx9ao6pdLUFoS3CSc98pwj1HCrmGHoVUB",
                    "TByQCQiBUtbLQNh6r1ZPNwBJC1jLgZjkuk"
                ]
            },
            "clientId": "guangxiang",
            "deviceType": "ANDROID",
            "sn": "guangxiang"
        }
        "#;

        let res = serde_json::from_str::<Message>(&raw);
        println!("res: {res:?}");
    }
}
