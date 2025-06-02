use wallet_database::{
    dao::bill::BillDao,
    entities::{
        bill::{BillKind, NewBillEntity},
        multisig_queue::MultisigQueueStatus,
    },
    factory::RepositoryFactory,
    repositories::{
        multisig_queue::MultisigQueueRepo, system_notification::SystemNotificationRepo,
    },
    DbPool,
};

use crate::{
    domain::multisig::MultisigQueueDomain,
    infrastructure::inner_event::InnerEvent,
    messaging::{
        notify::{event::NotifyEvent, transaction::AcctChangeFrontend, FrontendNotifyEvent},
        system_notification::{AccountType, Notification, NotificationType, TransactionStatus},
    },
    service::system_notification::SystemNotificationService,
};

// biz_type = ACCT_CHANGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AcctChange {
    // 交易hash
    pub tx_hash: String,
    // 链码
    pub chain_code: String,
    // 币种符号
    #[serde(deserialize_with = "wallet_utils::serde_func::deserialize_uppercase")]
    pub symbol: String,
    // 交易方式 0转入 1转出 2初始化
    pub transfer_type: i8,
    // 交易类型 1:普通交易，2:部署多签账号 3:服务费
    pub tx_kind: i8,
    // 发起方
    pub from_addr: String,
    // 接收方
    #[serde(default)]
    pub to_addr: String,
    // 合约地址
    #[serde(default)]
    pub token: Option<String>,
    // 交易额
    #[serde(default)]
    pub value: f64,
    // 手续费
    pub transaction_fee: f64,
    // 交易时间
    #[serde(default)]
    pub transaction_time: String,
    // 交易状态 true-成功 false-失败
    pub status: bool,
    // 是否多签 1-是，0-否
    #[serde(default)]
    pub is_multisig: i32,
    // 队列id
    #[serde(default)]
    pub queue_id: String,
    // 块高
    pub block_height: i64,
    // 备注
    #[serde(default)]
    pub notes: String,
    // 带宽消耗
    #[serde(default)]
    pub net_used: u64,
    // 能量消耗
    #[serde(default)]
    pub energy_used: Option<u64>,
}

// impl AcctChange {
//     pub(crate) fn name(&self) -> String {
//         "ACCT_CHANGE".to_string()
//     }
// }

impl TryFrom<&AcctChange> for NewBillEntity {
    type Error = crate::ServiceError;

    fn try_from(value: &AcctChange) -> Result<Self, Self::Error> {
        let tx_kind = BillKind::try_from(value.tx_kind)?;
        let status = if value.status { 2 } else { 3 };

        let consumer = wallet_chain_interact::BillResourceConsume::new_tron(
            value.net_used,
            value.energy_used.unwrap_or_default(),
        )
        .to_json_str()?;

        Ok(NewBillEntity {
            hash: value.tx_hash.clone(),
            chain_code: value.chain_code.clone(),
            symbol: value.symbol.clone(),
            tx_type: value.transfer_type,
            tx_kind,
            from: value.from_addr.clone(),
            to: value.to_addr.clone(),
            token: value.token.clone(),
            value: value.value,
            transaction_fee: value.transaction_fee.to_string(),
            transaction_time: wallet_utils::time::datetime_to_timestamp(&value.transaction_time),
            status,
            multisig_tx: value.is_multisig == 1,
            queue_id: value.queue_id.clone(),
            block_height: value.block_height.to_string(),
            notes: value.notes.clone(),
            signer: vec![],
            resource_consume: consumer,
        })
    }
}

impl From<&AcctChange> for AcctChangeFrontend {
    fn from(value: &AcctChange) -> Self {
        Self {
            tx_hash: value.tx_hash.clone(),
            chain_code: value.chain_code.clone(),
            symbol: value.symbol.clone(),
            transfer_type: value.transfer_type,
            tx_kind: value.tx_kind,
            from_addr: value.from_addr.clone(),
            to_addr: value.to_addr.clone(),
            token: value.token.clone(),
            value: value.value,
            transaction_fee: value.transaction_fee,
            transaction_time: value.transaction_time.clone(),
            status: value.status,
            is_multisig: value.is_multisig,
            queue_id: value.queue_id.clone(),
            block_height: value.block_height,
            notes: value.notes.clone(),
        }
    }
}

impl AcctChange {
    pub(crate) async fn exec(self, msg_id: &str) -> Result<(), crate::ServiceError> {
        // let event_name = self.name();
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        // tracing::info!(
        //     event_name = %event_name,
        //     ?self,
        //     "Starting AcctChange processing");

        // bill create
        let tx = NewBillEntity::try_from(&self)?;
        let tx_kind = tx.tx_kind;
        BillDao::create(tx, pool.as_ref()).await?;

        if !self.queue_id.is_empty() {
            Self::handle_queue(&self, &pool).await?;
        }

        // 更新资产,不进行新增(垃圾币)
        Self::sync_assets(&self).await?;

        // 创建系统通知
        if tx_kind.needs_system_notify() && self.value != 0.0 {
            Self::system_notification(msg_id, &self, &pool).await?;
        }

        // send acct_change to frontend
        let change_frontend = AcctChangeFrontend::from(&self);
        let data = NotifyEvent::AcctChange(change_frontend);
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }

    async fn handle_queue(change: &AcctChange, pool: &DbPool) -> Result<(), crate::ServiceError> {
        let status = if change.status {
            MultisigQueueStatus::Success
        } else {
            MultisigQueueStatus::Fail
        };

        MultisigQueueRepo::update_status_hash(&change.queue_id, status, &change.tx_hash, pool)
            .await?;

        // 多签队列不存在可以允许 不上报忽略
        let rs = MultisigQueueDomain::update_raw_data(&change.queue_id, pool.clone()).await;
        match rs {
            Ok(_) => {}
            Err(e) => {
                if !matches!(e, crate::ServiceError::Database(_)) {
                    return Err(e);
                };
                tracing::error!(%e, "acct_change update queue fail");
            }
        }

        Ok(())
    }

    async fn sync_assets(acct_change: &AcctChange) -> Result<(), crate::ServiceError> {
        if !acct_change.status {
            tracing::warn!("acct_change status is false, skip sync assets");
            return Ok(());
        }

        let inner_event_handle = crate::manager::Context::get_global_inner_event_handle()?;
        inner_event_handle.send(InnerEvent::SyncAssets {
            addr_list: vec![
                acct_change.from_addr.to_string(),
                acct_change.to_addr.to_string(),
            ],
            chain_code: acct_change.chain_code.to_string(),
            symbol: acct_change.symbol.to_string(),
        })?;
        // tracing::info!("发送同步资产事件");
        Ok(())
    }

    async fn system_notification(
        msg_id: &str,
        acct_change: &AcctChange,
        pool: &DbPool,
    ) -> Result<(), crate::ServiceError> {
        // 交易方式 0转入 1转出 2初始化
        let address = match acct_change.transfer_type {
            0 => acct_change.to_addr.as_str(),
            1 => acct_change.from_addr.as_str(),
            _ => return Ok(()),
        };

        // check system notification exists
        if SystemNotificationRepo::find_by_id(msg_id, pool)
            .await?
            .is_some()
        {
            tracing::warn!("system_noti already exists");
            return Ok(());
        }

        let (transaction_status, notification_type) =
            Self::get_notify_status(acct_change.transfer_type, acct_change.status)?;

        let account_type = if acct_change.is_multisig == 1 {
            AccountType::Multisig
        } else {
            AccountType::Regular
        };

        // build notify
        let notify = Notification::new_transaction_notification(
            account_type,
            "",
            address,
            &acct_change.from_addr,
            &acct_change.to_addr,
            acct_change.value,
            &acct_change.symbol,
            &acct_change.chain_code,
            &transaction_status,
            &acct_change.tx_hash,
            &notification_type,
        );
        let req = notify.gen_create_system_notification_entity(
            msg_id,
            0,
            Some("tx_hash".to_string()),
            Some(acct_change.tx_hash.to_string()),
        )?;

        let repo = RepositoryFactory::repo(pool.clone());
        let system_notification_service = SystemNotificationService::new(repo);

        system_notification_service
            .add_multi_system_notification_with_key_value(&vec![req])
            .await?;
        Ok(())
    }

    fn get_notify_status(
        transfer_type: i8,
        status: bool,
    ) -> Result<(TransactionStatus, NotificationType), crate::ServiceError> {
        let (transaction_status, notification_type) = match (transfer_type, status) {
            (0, true) => (
                TransactionStatus::Received,
                NotificationType::ReceiveSuccess,
            ),
            (1, true) => (TransactionStatus::Sent, NotificationType::TransferSuccess),
            (1, false) => (
                TransactionStatus::NotSent,
                NotificationType::TransferFailure,
            ),
            (_, _) => return Err(crate::ServiceError::Parameter("invaild status".to_string())),
        };

        Ok((transaction_status, notification_type))
    }
}

#[cfg(test)]
mod test {
    use crate::{messaging::mqtt::topics::AcctChange, test::env::get_manager};

    async fn init_manager() {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await.unwrap();
    }

    // 普通账交易
    #[tokio::test]
    async fn acct_change() -> anyhow::Result<()> {
        init_manager().await;

        // let change = r#"{"blockHeight":56520912,"chainCode":"tron","fromAddr":"TPgKQJDibjSNQ8AsXp35dja9gP1iLs7BdV","isMultisig":1,"queueId":"255844763314556930","signer":[],"status":true,"symbol":"trx","toAddr":"TGCya1Rrupsqv2RAr48er1B6bcJxK9TZnG","token":"","transactionFee":1.337,"transactionTime":"2025-04-29 10:16:51","transferType":1,"txHash":"e1e4412bf86c6d3b79b7da4838b1f26b3889bcdd62cc768ee3fe4b09778e3396","txKind":1,"value":10,"valueUsdt":2.4751915605691943}"#;
        let change = r#"{"txHash":"9gaiYphWbxjabMk2L4ALVXiq2NRgwRkmdAUFsW8pj6qw","chainCode":"sui","symbol":"SUI","transferType":1,"txKind":1,"fromAddr":"0xfba1550112b16f3608669c8ab4268366c7bacb3a2cb844594ad67c21af85a1dd","toAddr":"0x427e388a85bbb00da32af7716b15749a147affb8ed3a87cd7ce1f98f0b20bc6f","token":null,"value":0.01,"transactionFee":0.00173788,"transactionTime":"2025-05-29 03:45:50","status":true,"isMultisig":0,"queueId":"","blockHeight":150642276,"notes":"","netUsed":0,"energyUsed":null}"#;
        let change = serde_json::from_str::<AcctChange>(&change).unwrap();

        let _res = change.exec("1").await.unwrap();
        Ok(())
    }
}
