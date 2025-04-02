use wallet_database::{
    dao::{
        bill::BillDao, multisig_account::MultisigAccountDaoV1, multisig_queue::MultisigQueueDaoV1,
    },
    entities::{
        bill::{BillKind, NewBillEntity},
        chain::ChainEntity,
        multisig_queue::MultisigQueueStatus,
    },
    factory::RepositoryFactory,
    repositories::{
        account::AccountRepoTrait, multisig_queue::MultisigQueueRepo, permission::PermissionRepo,
    },
};

use crate::{
    domain::{self, multisig::MultisigDomain},
    service::{
        account::AccountService, asset::AssetsService,
        system_notification::SystemNotificationService,
    },
    system_notification::{AccountType, Notification, NotificationType, TransactionStatus},
};

use super::AcctChange;

/*
    转入
    {
    "appId": "",
    "bizType": "ACCT_CHANGE",
    "body": {
        "blockHeight": 65590176,
        "chainCode": "tron",
        "fromAddr": "TTofbJMU2iMRhA39AJh51sYvhguWUnzeB1",
        "isMultisig": 0,
        "status": true,
        "symbol": "trx",
        "toAddr": "TLzteCJi4jSGor5EDRYZcdQ4hsZRQQZ4XR",
        "token": "",
        "transactionFee": 0,
        "transactionTime": "2024-09-27 14:34:42",
        "transferType": 0,
        "txHash": "fed8f3933cfd972f69c9a2c8b322fac853dc1b377b19c40c4c1c5bb5a2c5fa89",
        "txKind": 1,
        "value": 10
    },
    "clientId": "dec32245ec791966f00e56281100f7e1ab1fc23e819c906d39d0b22400e9a7b5",
    "deviceType": "ANDROID",
    "sn": "dec32245ec791966f00e56281100f7e1ab1fc23e819c906d39d0b22400e9a7b5"
    }

*/
impl AcctChange {
    pub(crate) async fn exec(self, msg_id: &str) -> Result<(), crate::ServiceError> {
        let event_name = self.name();
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting AcctChange processing");
        let AcctChange {
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
            ref queue_id,
            block_height,
            ref notes,
            net_used,
            energy_used,
            signer,
        } = self;

        let mut _status = if status { 2 } else { 3 };
        let timestamp = wallet_utils::time::datetime_to_timestamp(transaction_time);
        // 交易方式 0转入 1转出 2初始化
        let address = match transfer_type {
            0 => to_addr,
            1 => from_addr,
            _ => return Ok(()),
        };

        let consumer = wallet_chain_interact::BillResourceConsume::new_tron(
            net_used,
            energy_used.unwrap_or_default(),
        )
        .to_json_str()?;

        let tx_kind_enum = BillKind::try_from(tx_kind)?;
        let multisig_tx = is_multisig == 1;
        let bill_params: NewBillEntity = NewBillEntity::new(
            tx_hash.to_owned(),
            from_addr.to_string(),
            to_addr.to_string(),
            value,
            chain_code.to_string(),
            symbol.to_string(),
            multisig_tx,
            tx_kind_enum,
            notes.to_string(),
        )
        .with_queue_id(queue_id)
        .with_status(_status)
        .with_token(&(token.clone()).unwrap_or_default())
        .with_tx_type(transfer_type)
        .with_block_height(&block_height.to_string())
        .with_transaction_fee(&transaction_fee.to_string())
        .with_transaction_time(timestamp)
        .with_resource_consume(&consumer)
        .with_signer(signer);
        BillDao::create(bill_params, pool.as_ref()).await?;
        tracing::info!(
            event_name = %event_name,
            "BillDao::create bill_params"
        );
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
        tracing::info!(
            event_name = %event_name,
            "MultisigQueueDaoV1::update_status_and_tx_hash"
        );
        // 添加或更新资产余额
        Self::upsert_than_sync_assets(from_addr, to_addr, chain_code, symbol).await?;

        tracing::info!(
            event_name = %event_name,
            "Self::upsert_than_sync_assets"
        );
        Self::create_system_notification(
            msg_id,
            tx_hash,
            symbol,
            chain_code,
            value,
            is_multisig,
            tx_kind_enum,
            transfer_type,
            status,
            address,
            queue_id,
        )
        .await?;

        // 发送账变通知
        let data = crate::notify::NotifyEvent::AcctChange(
            crate::notify::event::transaction::AcctChangeFrontend {
                tx_hash: tx_hash.to_string(),
                chain_code: chain_code.to_string(),
                symbol: symbol.to_string(),
                transfer_type,
                tx_kind,
                from_addr: from_addr.to_string(),
                to_addr: to_addr.to_string(),
                token: token.clone(),
                value,
                transaction_fee,
                transaction_time: transaction_time.to_string(),
                status,
                is_multisig,
                queue_id: queue_id.to_string(),
                block_height,
                notes: notes.to_string(),
            },
        );
        crate::notify::FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }

    async fn upsert_than_sync_assets(
        from_addr: &str,
        to_addr: &str,
        chain_code: &str,
        symbol: &str,
    ) -> Result<(), crate::ServiceError> {
        let asset_list = vec![from_addr.to_string(), to_addr.to_string()];

        tracing::info!(
            event_name = "AcctChange.upsert_than_sync_assets",
            ?asset_list,
            ?chain_code,
            ?symbol,
            "Start upsert_than_sync_assets"
        );
        if !asset_list.is_empty() {
            let pool = crate::manager::Context::get_global_sqlite_pool()?;
            let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

            AssetsService::new(repo)
                .sync_assets_by_addr(
                    asset_list,
                    Some(chain_code.to_string()),
                    vec![symbol.to_string()],
                )
                .await?;
        }

        Ok(())
    }

    async fn create_system_notification(
        msg_id: &str,
        tx_hash: &str,
        symbol: &str,
        chain_code: &str,
        value: f64,
        is_multisig: i32,
        tx_kind_enum: BillKind,
        transfer_type: i8,
        status: bool,
        address: &str,
        queue_id: &str,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let repo = RepositoryFactory::repo(pool.clone());
        let mut account_service = AccountService::new(repo);

        let repo = RepositoryFactory::repo(pool.clone());
        let mut system_notification_service = SystemNotificationService::new(repo);
        if wallet_database::repositories::system_notification::SystemNotificationRepoTrait::detail(
            &mut system_notification_service.repo,
            msg_id,
        )
        .await?
        .is_some()
        {
            tracing::warn!("system_noti already exists");
            return Ok(());
        };

        // 判断是否是手续费，如果是则不创建通知
        if let Some(chain) = ChainEntity::detail(&*pool, chain_code).await? {
            if chain.main_symbol == *symbol && value == f64::default() {
                tracing::warn!("tx_hash:{} is fee", tx_hash);
                return Ok(());
            }
        }

        // 添加系统通知
        // type: 1/ 普通账户 2/多签账户建立 4/多签转账成功

        // 交易类型 1:普通交易，2:部署多签账号手续费 3:服务费
        // 是否多签 1-是，0-否
        let reqs = match (is_multisig, tx_kind_enum) {
            // 非多签的普通交易
            (0, BillKind::Transfer) => {
                let mut reqs = Vec::new();
                // transfer_type: 交易方式 0转入 1转出 2初始化
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
                    (_, _) => return Ok(()),
                };

                // 如果有查询是否是自己的账户
                if let Some(account) = account_service
                    .repo
                    .detail_by_address_and_chain_code(address, chain_code)
                    .await?
                {
                    let notif = Notification::new_transaction_notification(
                        AccountType::Regular,
                        &account.name,
                        &account.address,
                        value,
                        symbol,
                        chain_code,
                        &transaction_status,
                        tx_hash,
                        &notification_type,
                    );
                    let req = notif.gen_create_system_notification_entity(
                        msg_id,
                        0,
                        Some("tx_hash".to_string()),
                        Some(tx_hash.to_string()),
                    )?;
                    reqs.push(req);
                }

                reqs
            }
            // 部署多签账号手续费
            (_, BillKind::DeployMultiSign) => {
                tracing::warn!("deploy multisig account fee");
                return Ok(());
            }
            // 服务费
            (_, BillKind::ServiceCharge) => {
                tracing::warn!("service charge");
                return Ok(());
            }
            // 多签的普通交易
            (1, BillKind::Transfer) => {
                let mut reqs = Vec::new();
                // transfer_type: 交易方式 0转入 1转出 2初始化
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
                    (_, _) => {
                        tracing::warn!("invalid transfer type or status");
                        return Ok(());
                    }
                };
                tracing::warn!("multisig account: address: {address}");

                let queue = MultisigQueueRepo::find_by_id(&pool, queue_id)
                    .await
                    .map_err(crate::ServiceError::Database)?;

                let Some(account) = queue else {
                    tracing::warn!("multisig queue not found");
                    return Ok(());
                };

                let name = if !account.permission_id.is_empty() {
                    let permission =
                        PermissionRepo::find_by_id(&pool, &account.permission_id).await?;
                    permission.name
                } else if !account.account_id.is_empty() {
                    if MultisigAccountDaoV1::find_by_address(address, pool.as_ref())
                        .await
                        .map_err(crate::ServiceError::Database)?
                        .is_none()
                    {
                        let mut repo = RepositoryFactory::repo(pool.clone());
                        MultisigDomain::recover_multisig_data_by_address(&mut repo, address)
                            .await?;
                    };
                    let Some(multisig_account) =
                        MultisigAccountDaoV1::find_by_address(address, pool.as_ref()).await?
                    else {
                        tracing::warn!("multisig account not found");
                        return Ok(());
                    };

                    multisig_account.name
                } else {
                    tracing::warn!("multisig account not found");
                    return Ok(());
                };

                tracing::warn!("multisig account: name: {}", name);
                let notif = Notification::new_transaction_notification(
                    AccountType::Multisig,
                    &name,
                    address,
                    value,
                    symbol,
                    chain_code,
                    &transaction_status,
                    tx_hash,
                    &notification_type,
                );
                let req = notif.gen_create_system_notification_entity(
                    msg_id,
                    0,
                    Some("tx_hash".to_string()),
                    Some(tx_hash.to_string()),
                )?;

                reqs.push(req);
                reqs
            }
            _ => {
                tracing::warn!("unknown tx_kind_enum");
                return Ok(());
            }
        };

        let pool = crate::Context::get_global_sqlite_pool()?;
        let repo = RepositoryFactory::repo(pool.clone());
        let system_notification_service = SystemNotificationService::new(repo);

        system_notification_service
            .add_multi_system_notification_with_key_value(&reqs)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{mqtt::payload::incoming::transaction::AcctChange, test::env::get_manager};

    #[tokio::test]
    async fn acct_change() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        // let str = r#"{"blockHeight":21391939,"chainCode":"eth","fromAddr":"0x1457a81B300cB106187Dd227b0319E2a851BAb24","isMultisig":0,"status":true,"symbol":"eth","toAddr":"0x7B3123AA8Cf1137Da498f3d581aD3B16a9DC55a9","token":"","transactionFee":0,"transactionTime":"2024-12-13 06:56:35","transferType":0,"txHash":"0xb8fb5be8584735a0fbb2a9fd8e3a1b7fd1f003203c719d23561c5e679bb5490d","txKind":1,"value":0.00011,"valueUsdt":0.42906098761791545}"#;
        // let str1 = r#"{"blockHeight":21391939,"chainCode":"eth","fromAddr":"TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1","isMultisig":0,"status":true,"symbol":"eth","toAddr":"TTofbJMU2iMRhA39AJh51sYvhguWUnzeB1","token":"","transactionFee":0,"transactionTime":"2024-12-13 06:56:35","transferType":1,"txHash":"ef0e324526c8647a9a480ff41fd8271c85742061c223d522c11a4e18c3c1a87a","txKind":1,"value":0.00011,"valueUsdt":0.42906098761791545}"#;
        // let str1 = r#"{"blockHeight":70835889,"chainCode":"tron","fromAddr":"TGtSVaqXzzGM2XgbUvgZzZeNqFwp1VvyXS","isMultisig":0,"queueId":"","signer":["TAqUJ9enU8KkZYySA51iQim7TxbbdLR2wn"],"status":true,"symbol":"trx","toAddr":"TLCdjLpnWynE7pyT34PQHwv7mXP3GAEPaZ","transactionFee":0.269,"transactionTime":"2025-03-28 11:21:51","transferType":1,"txHash":"6b699d41b3dc42c9e6a461780b48117a6d9a099daffac4dad57324655277d5f8","txKind":1,"value":1,"valueUsdt":0.22988765032767838}"#;
        let str1 = r#"{"txHash":"0x19fa3bd522cd9dd181c8cd14adb6cc626ce322c4296d9ff255a322201762c0a9","chainCode":"eth","symbol":"USDT","transferType":1,"txKind":1,"fromAddr":"0xd8Dda100cD442D755407E3D976b9DFfE24e11164","toAddr":"0x6E39F7A298A6523B5785e103e007f9bD37C9DCF2","token":"0xdAC17F958D2ee523a2206206994597C13D831ec7","value":2.12565,"transactionFee":0.000032147019247989,"transactionTime":"2025-04-01 03:18:47","status":true,"isMultisig":0,"queueId":"","blockHeight":22171324,"notes":""}"#;
        let changet = serde_json::from_str::<AcctChange>(&str1).unwrap();

        let res = changet.exec("1").await;
        println!("{:?}", res);
        Ok(())
    }
}
