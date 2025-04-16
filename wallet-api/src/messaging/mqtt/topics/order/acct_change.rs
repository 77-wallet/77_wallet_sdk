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
    messaging::{
        notify::{event::NotifyEvent, transaction::AcctChangeFrontend, FrontendNotifyEvent},
        system_notification::{AccountType, Notification, NotificationType, TransactionStatus},
    },
    service::{asset::AssetsService, system_notification::SystemNotificationService},
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

impl AcctChange {
    pub(crate) fn name(&self) -> String {
        "ACCT_CHANGE".to_string()
    }
}

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
        let event_name = self.name();
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting AcctChange processing");

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

        Ok(MultisigQueueDomain::update_raw_data(&change.queue_id, pool.clone()).await?)
    }

    async fn sync_assets(acct_change: &AcctChange) -> Result<(), crate::ServiceError> {
        let asset_list = vec![
            acct_change.from_addr.to_string(),
            acct_change.to_addr.to_string(),
        ];

        if !asset_list.is_empty() {
            let pool = crate::manager::Context::get_global_sqlite_pool()?;
            let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

            AssetsService::new(repo)
                .sync_assets_by_addr(
                    asset_list,
                    Some(acct_change.chain_code.to_string()),
                    vec![acct_change.symbol.to_string()],
                )
                .await?;
        }

        Ok(())
    }

    async fn system_notification(
        msg_id: &str,
        acct_change: &AcctChange,
        pool: &DbPool,
    ) -> Result<(), crate::ServiceError> {
        // 交易方式 0转入 1转出 2初始化
        // let address = match acct_change.transfer_type {
        //     0 => acct_change.to_addr.as_str(),
        //     1 => acct_change.from_addr.as_str(),
        //     _ => return Ok(()),
        // };

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

        let change = r#"{"blockHeight":21391939,"chainCode":"eth","fromAddr":"TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1","isMultisig":0,"status":true,"symbol":"eth","toAddr":"TTofbJMU2iMRhA39AJh51sYvhguWUnzeB1","token":"","transactionFee":0,"transactionTime":"2024-12-13 06:56:35","transferType":1,"txHash":"ef0e324526c8647a9a480ff41fd8271c85742061c223d522c11a4e18c3c1a87a","txKind":1,"value":0.00011,"valueUsdt":0.42906098761791545}"#;
        let change = serde_json::from_str::<AcctChange>(&change).unwrap();

        let _res = change.exec("1").await.unwrap();
        Ok(())
    }

    // 多签交易

    // 资源变更
}

// async fn create_system_notification(
//     msg_id: &str,
//     tx_hash: &str,
//     symbol: &str,
//     chain_code: &str,
//     value: f64,
//     is_multisig: i32,
//     tx_kind_enum: BillKind,
//     transfer_type: i8,
//     status: bool,
//     address: &str,
// ) -> Result<(), crate::ServiceError> {
//     let pool = crate::manager::Context::get_global_sqlite_pool()?;
//     let repo = RepositoryFactory::repo(pool.clone());
//     let mut account_service = AccountService::new(repo);

//     let repo = RepositoryFactory::repo(pool.clone());
//     let mut system_notification_service = SystemNotificationService::new(repo);
//     if wallet_database::repositories::system_notification::SystemNotificationRepoTrait::detail(
//         &mut system_notification_service.repo,
//         msg_id,
//     )
//     .await?
//     .is_some()
//     {
//         tracing::warn!("system_noti already exists");
//         return Ok(());
//     };

//     // 判断是否是手续费，如果是则不创建通知
//     if let Some(chain) = ChainEntity::detail(&*pool, chain_code).await? {
//         if chain.main_symbol == *symbol && value == f64::default() {
//             tracing::warn!("tx_hash:{} is fee", tx_hash);
//             return Ok(());
//         }
//     }

//     // 添加系统通知
//     // type: 1/ 普通账户 2/多签账户建立 4/多签转账成功

//     // 交易类型 1:普通交易，2:部署多签账号手续费 3:服务费
//     // 是否多签 1-是，0-否
//     let reqs = match (is_multisig, tx_kind_enum) {
//         // 非多签的普通交易
//         (0, BillKind::Transfer) => {
//             let mut reqs = Vec::new();
//             // transfer_type: 交易方式 0转入 1转出 2初始化
//             let (transaction_status, notification_type) = match (transfer_type, status) {
//                 (0, true) => (
//                     TransactionStatus::Received,
//                     NotificationType::ReceiveSuccess,
//                 ),
//                 (1, true) => (TransactionStatus::Sent, NotificationType::TransferSuccess),
//                 (1, false) => (
//                     TransactionStatus::NotSent,
//                     NotificationType::TransferFailure,
//                 ),
//                 (_, _) => return Ok(()),
//             };

//             // 如果有查询是否是自己的账户
//             if let Some(account) = account_service
//                 .repo
//                 .detail_by_address_and_chain_code(address, chain_code)
//                 .await?
//             {
//                 let notif = Notification::new_transaction_notification(
//                     AccountType::Regular,
//                     &account.name,
//                     &account.address,
//                     value,
//                     symbol,
//                     chain_code,
//                     &transaction_status,
//                     tx_hash,
//                     &notification_type,
//                 );
//                 let req = notif.gen_create_system_notification_entity(
//                     msg_id,
//                     0,
//                     Some("tx_hash".to_string()),
//                     Some(tx_hash.to_string()),
//                 )?;
//                 reqs.push(req);
//             }

//             reqs
//         }
//         // 部署多签账号手续费
//         (_, BillKind::DeployMultiSign) => {
//             tracing::warn!("deploy multisig account fee");
//             return Ok(());
//         }
//         // 服务费
//         (_, BillKind::ServiceCharge) => {
//             tracing::warn!("service charge");
//             return Ok(());
//         }
//         // 多签的普通交易
//         (1, BillKind::Transfer) => {
//             let mut reqs = Vec::new();
//             // transfer_type: 交易方式 0转入 1转出 2初始化
//             let (transaction_status, notification_type) = match (transfer_type, status) {
//                 (0, true) => (
//                     TransactionStatus::Received,
//                     NotificationType::ReceiveSuccess,
//                 ),
//                 (1, true) => (TransactionStatus::Sent, NotificationType::TransferSuccess),
//                 (1, false) => (
//                     TransactionStatus::NotSent,
//                     NotificationType::TransferFailure,
//                 ),
//                 (_, _) => {
//                     tracing::warn!("invalid transfer type or status");
//                     return Ok(());
//                 }
//             };
//             tracing::warn!("multisig account: address: {address}");
//             if MultisigAccountDaoV1::find_by_address(address, pool.as_ref())
//                 .await
//                 .map_err(crate::ServiceError::Database)?
//                 .is_none()
//             {
//                 let mut repo = RepositoryFactory::repo(pool.clone());
//                 MultisigDomain::recover_multisig_data_by_address(&mut repo, address).await?;
//             }

//             if let Some(multisig_account) =
//                 MultisigAccountDaoV1::find_by_address(address, pool.as_ref()).await?
//             {
//                 tracing::warn!("multisig account: name: {}", multisig_account.name);
//                 let notif = Notification::new_transaction_notification(
//                     AccountType::Multisig,
//                     &multisig_account.name,
//                     address,
//                     value,
//                     symbol,
//                     chain_code,
//                     &transaction_status,
//                     tx_hash,
//                     &notification_type,
//                 );
//                 let req = notif.gen_create_system_notification_entity(
//                     msg_id,
//                     0,
//                     Some("tx_hash".to_string()),
//                     Some(tx_hash.to_string()),
//                 )?;

//                 reqs.push(req);
//             };

//             reqs
//         }
//         _ => {
//             tracing::warn!("unknown tx_kind_enum");
//             return Ok(());
//         }
//     };

//     let pool = crate::Context::get_global_sqlite_pool()?;
//     let repo = RepositoryFactory::repo(pool.clone());
//     let system_notification_service = SystemNotificationService::new(repo);

//     system_notification_service
//         .add_multi_system_notification_with_key_value(&reqs)
//         .await?;
//     Ok(())
// }
