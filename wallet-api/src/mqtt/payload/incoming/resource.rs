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
    service::{asset::AssetsService, system_notification::SystemNotificationService},
    system_notification::{Notification, NotificationType},
};

// biz_type = TRON_SIGN_FREEZE_DELEGATE_VOTE_CHANGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TronSignFreezeDelegateVoteChange {
    // 交易hash
    pub tx_hash: String,
    // 链编码
    pub chain_code: String,
    // 币种符号
    pub symbol: String,
    // 交易方式 0转入 1转出 2初始化
    pub transfer_type: i8,
    // 交易类型 1:普通交易，2:部署多签账号 3:服务费
    pub tx_kind: BillKind,
    // from地址
    pub from_addr: String,
    // to地址
    #[serde(default)]
    pub to_addr: String,
    // 合约地址
    pub token: Option<String>,
    // 交易额
    pub value: f64,
    // 交易额-usdt
    #[serde(default)]
    pub value_usdt: Option<f64>,
    // 手续费
    pub transaction_fee: f64,
    // 交易时间
    pub transaction_time: String,
    // 交易状态
    pub status: bool,
    // 是否多签 1-是，0-否
    pub is_multisig: i32,
    // 块高
    pub block_height: i64,
    // 备注
    #[serde(default)]
    pub notes: String,
    // 业务id
    pub queue_id: String,
    // 带宽消耗
    #[serde(default)]
    pub net_used: u64,
    // 能量消耗
    #[serde(default)]
    pub energy_used: Option<u64>,
    // BANDWIDTH  / ENERGY
    #[serde(default)]
    pub resource: Option<String>,
    // 是否锁定
    #[serde(default)]
    pub lock: Option<bool>,
    // 锁定周期
    #[serde(default)]
    pub lock_period: Option<String>,
    // 投票的节点信息
    #[serde(default)]
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
    // 投票的地址
    pub vote_address: String,
    // 票数
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
            mut value,
            transaction_fee,
            ref transaction_time,
            status,
            is_multisig,
            block_height,
            ref notes,
            ref queue_id,
            net_used,
            energy_used,
            ref votes,
            ..
        } = self;
        let mut _status = if status { 2 } else { 3 };
        let timestamp = wallet_utils::time::datetime_to_timestamp(transaction_time);

        let consumer = wallet_chain_interact::BillResourceConsume::new_tron(
            net_used,
            energy_used.unwrap_or_default(),
        )
        .to_json_str()?;

        if !votes.is_empty() {
            value = 0f64;
            for vote in votes.iter() {
                value += vote.vote_count as f64;
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

        // 添加或更新资产余额
        Self::upsert_than_sync_assets(from_addr, to_addr).await?;
        Self::create_system_notification(
            msg_id,
            &self.tx_hash,
            &self.from_addr,
            self.tx_kind,
            self.status,
        )
        .await?;

        let data = crate::notify::NotifyEvent::ResourceChange(self.into());
        crate::notify::FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }

    async fn upsert_than_sync_assets(
        from_addr: &str,
        to_addr: &str,
    ) -> Result<(), crate::ServiceError> {
        let asset_list = if to_addr.is_empty() {
            vec![from_addr.to_string()]
        } else {
            vec![from_addr.to_string(), to_addr.to_string()]
        };

        if !asset_list.is_empty() {
            let pool = crate::manager::Context::get_global_sqlite_pool()?;
            let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

            AssetsService::new(repo)
                .sync_assets_by_addr(asset_list, None, vec![])
                .await?;
        }

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
                &multisig_account.name,
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

    use crate::test::env::get_manager;

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

    #[tokio::test]
    async fn resource_change() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        // let str1 = r#"{"blockHeight":69741653,"chainCode":"tron","fromAddr":"TVx7Pi8Ftgzd7AputaoLidBR3Vb9xKfhqY","isMultisig":1,"netUsed":320.0,"queueId":"230495803410616320","status":true,"symbol":"trx","transactionFee":1000000.0,"transactionTime":"2025-02-18 19:14:12","transferType":1,"txHash":"0df3ba525f4688e73c25d3a15b26e7318054f25bb600c25fa52323fc9efa5e57","txKind":6,"value":6000000.0}"#;
        let str1 = r#"{"appId":"18071adc038afff6630","bizType":"TRON_SIGN_FREEZE_DELEGATE_VOTE_CHANGE","body":{"blockHeight":69789938,"chainCode":"tron","fromAddr":"TRbHD77Y6WWDaz9X5esrVKwEVwRM4gTw6N","isMultisig":1,"netUsed":317,"queueId":"231100718596100096","status":true,"symbol":"trx","transactionFee":1,"transactionTime":"2025-02-20 11:29:09","transferType":1,"txHash":"864b9491ba4551cc33379cf5d32a20ec1d366f3ccbca8b262aecd09b101532d0","txKind":6,"value":1},"clientId":"ce1eeb921f1205027699eecf78bbdc91","deviceType":"ANDROID","sn":"9920759727e10fd5204cfc1b8d54c79381190d6dc1c9db9cda30959545ad45c1","msgId":"67b6a1bfaff07a6a2fdd8fcb"}"#;

        let changet =
            serde_json::from_str::<crate::mqtt::payload::incoming::Message>(&str1).unwrap();

        tracing::warn!("change: {changet:#?}");
        // let res = changet.exec("1").await;
        // println!("{:?}", res);
        Ok(())
    }
}
