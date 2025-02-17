/*
{
    "clientId": "wenjing",
    "sn": "wenjing",
    "deviceType": "ANDROID",
    "bizType": "INIT",
    "body": [
        {
            "address": "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ",
            "balance": 4000002,
            "chainCode": "tron",
            "code": "sadsadsad",
              "tokenAddress": "",
              "decimals": 6
        }
    ]
}
*/

use wallet_database::{
    dao::{
        bill::BillDao, multisig_account::MultisigAccountDaoV1, multisig_queue::MultisigQueueDaoV1,
    },
    entities::{
        bill::{BillKind, NewBillEntity},
        multisig_queue::MultisigQueueStatus,
    },
    factory::RepositoryFactory,
};

use crate::{
    domain,
    service::system_notification::SystemNotificationService,
    system_notification::{Notification, NotificationType},
};

// biz_type = TRON_SIGN_FREEZE_DELEGATE_VOTE_CHANGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TronSignFreezeDelegateVoteChange {
    pub tx_hash: String,
    pub chain_code: String,
    pub symbol: String,
    pub transfer_type: i8,
    pub tx_kind: BillKind,
    pub from_addr: String,
    pub to_addr: String,
    pub token: Option<String>,
    pub value: f64,
    pub value_usdt: f64,
    pub transaction_fee: f64,
    pub transaction_time: String,
    pub status: bool,
    pub is_multisig: i32,
    pub block_height: i64,
    pub notes: String,
    pub queue_id: String,
    pub net_used: f64,
    pub energy_used: f64,
    pub resource: String,
    pub lock: bool,
    pub lock_period: String,
    pub votes: Vec<Vote>,
}

impl TronSignFreezeDelegateVoteChange {
    pub(crate) fn name(&self) -> String {
        "TRON_SIGN_FREEZE_DELEGATE_VOTE_CHANGE".to_string()
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Vote {
    pub vote_address: String,
    pub vote_count: u32,
}

// biz_type = TRON_SIGN_FREEZE_DELEGATE_VOTE_CHANGE
impl TronSignFreezeDelegateVoteChange {
    pub(crate) async fn exec(self, msg_id: &str) -> Result<(), crate::ServiceError> {
        let event_name = self.name();
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting TronSignFreezeDelegateVoteChange processing");

        let TronSignFreezeDelegateVoteChange {
            ref tx_hash,
            ref chain_code,
            ref symbol,
            transfer_type,
            tx_kind,
            ref from_addr,
            ref to_addr,
            ref token,
            value,
            transaction_fee,
            ref transaction_time,
            status,
            is_multisig,
            block_height,
            ref notes,
            ref queue_id,
            ..
        } = self;
        let mut _status = if status { 2 } else { 3 };
        let timestamp = wallet_utils::time::datetime_to_timestamp(transaction_time);

        // 主动查询链上的交易信息,获取交易所消耗的资源,以及更新状态
        let mut consumer = String::new();
        match domain::bill::BillDomain::get_onchain_bill(tx_hash, chain_code).await {
            Ok(res) => {
                if let Some(res) = res {
                    // _status = res.status;
                    consumer = res.resource_consume;
                    // transaction_fee = res.transaction_fee;
                }
            }
            Err(e) => {
                tracing::error!("mqtt get bill resource consumer error:{e:?}");
            }
        }

        let tx_kind_enum = tx_kind;
        let multisig_tx = is_multisig == 1;
        let bill_params: NewBillEntity = NewBillEntity::new(
            tx_hash.to_owned(),
            from_addr.to_string(),
            to_addr.to_string(),
            value,
            chain_code.to_string(),
            symbol.to_string(),
            multisig_tx,
            tx_kind_enum.clone(),
            notes.to_string(),
        )
        .with_queue_id(queue_id)
        .with_status(_status)
        .with_token(&(token.clone()).unwrap_or_default())
        .with_tx_type(transfer_type)
        .with_block_height(&block_height.to_string())
        .with_transaction_fee(&transaction_fee.to_string())
        .with_transaction_time(timestamp)
        .with_resource_consume(&consumer);
        BillDao::create(bill_params, pool.as_ref()).await?;

        if !queue_id.is_empty() {
            let q_status = if _status == 2 {
                MultisigQueueStatus::Success
            } else {
                MultisigQueueStatus::Fail
            };
            MultisigQueueDaoV1::update_status_and_tx_hash(queue_id, q_status, tx_hash, &*pool)
                .await
                .map_err(|e| crate::ServiceError::Database(e.into()))?;

            domain::multisig::queue::MultisigQueueDomain::update_raw_data(queue_id, pool.clone())
                .await?;
        }
        Self::create_system_notification(
            msg_id,
            &self.tx_hash,
            &self.from_addr,
            self.tx_kind,
            self.status,
        )
        .await?;

        let data = crate::notify::NotifyEvent::TronSignFreezeDelegateVoteChange(self.into());
        crate::notify::FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }

    async fn create_system_notification(
        msg_id: &str,
        tx_hash: &str,
        from_addr: &str,
        tx_kind: BillKind,
        status: bool,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let repo = RepositoryFactory::repo(pool.clone());
        if let Some(multisig_account) =
            MultisigAccountDaoV1::find_by_address(from_addr, pool.as_ref())
                .await
                .map_err(crate::ServiceError::Database)?
        {
            // 系统通知
            let notification = Notification::new_resource_notification(
                &multisig_account.address,
                &multisig_account.id,
                tx_kind,
                status,
                tx_hash,
                &NotificationType::ResourceChange,
            );
            let req = notification.gen_create_system_notification_entity(
                msg_id,
                0,
                Some("tx_hash".to_string()),
                Some(tx_hash.to_string()),
            )?;
            let system_notification_service = SystemNotificationService::new(repo);
            system_notification_service
                .add_multi_system_notification_with_key_value(&[req])
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    #[test]
    fn test_decimal() {
        let balance = wallet_types::Decimal::from_str("1996.733").unwrap();
        let balance = wallet_utils::unit::convert_to_u256(&balance.to_string(), 6).unwrap();
        println!("balance: {balance}");
        println!(
            "balance: {}",
            wallet_utils::unit::format_to_string(balance, 6).unwrap()
        );
        // let balance = wallet_utils::unit::u256_from_str(&balance.to_string()).unwrap();
    }
}
