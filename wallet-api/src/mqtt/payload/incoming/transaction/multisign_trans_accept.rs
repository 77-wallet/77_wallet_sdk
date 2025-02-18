use crate::{
    domain::multisig::MultisigDomain,
    service::system_notification::SystemNotificationService,
    system_notification::{Notification, NotificationType},
};
use wallet_database::{
    dao::{
        multisig_account::MultisigAccountDaoV1, multisig_queue::MultisigQueueDaoV1,
        multisig_signatures::MultisigSignatureDaoV1,
    },
    entities::{
        bill::BillKind,
        multisig_queue::{MultisigQueueStatus, NewMultisigQueueEntity},
    },
    factory::RepositoryFactory,
    repositories::multisig_queue::MultisigQueueRepo,
};

// 多签交易队列的创建 同步给所有人
use super::MultiSignTransAccept;

/*
    {
        "clientId": "wenjing",
        "sn": "device460",
        "deviceType": "typeE",
        "bizType": "MULTI_SIGN_TRANS_ACCEPT",
        "body": {
            "id": "tx123456789",
            "fromAddr": "THx9ao6pdLUFoS3CSc98pwj1HCrmGHoVUB",
            "toAddr": "0xReceiverAddress",
            "value": "1000",
            "expiration": 1698806400,
            "symbol": "eth",
            "chainCode": "eth",
            "tokenAddr": null,
            "msgHash": "0xMessageHash",
            "txHash": "0xTransactionHash",
            "rawData": "raw transaction data",
            "status": 0,
            "notes": "This is a test transaction",
            "createdAt": "2024-07-30T12:34:56Z"
        }
    }
*/

//MultiSignTransAccept
impl From<&MultiSignTransAccept> for NewMultisigQueueEntity {
    fn from(value: &MultiSignTransAccept) -> Self {
        Self {
            id: value.id.clone(),
            from_addr: value.from_addr.clone(),
            to_addr: value.to_addr.clone(),
            value: value.value.clone(),
            symbol: value.symbol.clone(),
            expiration: value.expiration,
            chain_code: value.chain_code.clone(),
            token_addr: value.token_addr.clone(),
            msg_hash: value.msg_hash.clone(),
            tx_hash: value.tx_hash.clone(),
            raw_data: value.raw_data.clone(),
            status: MultisigQueueStatus::PendingSignature,
            notes: value.notes.clone(),
            signatures: vec![],
            fail_reason: "".to_string(),
            account_id: value.account_id.clone(),
            create_at: value.created_at,
            transfer_type: BillKind::try_from(value.transfer_type).unwrap(),
        }
    }
}

impl MultiSignTransAccept {
    pub(crate) async fn exec(self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let event_name = self.name();
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let repo = RepositoryFactory::repo(pool.clone());
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting MultiSignTransAccept processing");
        let MultiSignTransAccept {
            ref id,
            ref from_addr,
            ref to_addr,
            ref value,
            expiration,
            ref symbol,
            ref chain_code,
            ref token_addr,
            ref msg_hash,
            ref tx_hash,
            ref raw_data,
            status,
            ref notes,
            created_at,
            ref signatures,
            ref account_id,
            transfer_type,
        } = self;
        // 新增交易队列数据
        let params: NewMultisigQueueEntity = (&self).into();

        let _res = MultisigQueueDaoV1::create_queue(&params, pool.as_ref())
            .await
            .map_err(|e| crate::ServiceError::Database(e.into()))?;

        for sig in signatures {
            MultisigSignatureDaoV1::create_or_update(sig, pool.clone())
                .await
                .map_err(|e| crate::ServiceError::Database(e.into()))?;
        }

        if MultisigAccountDaoV1::find_by_address(from_addr, pool.as_ref())
            .await
            .map_err(crate::ServiceError::Database)?
            .is_none()
        {
            MultisigDomain::recover_multisig_account_by_id(id).await?;
        }

        if let Some(multisig_account) =
            MultisigAccountDaoV1::find_by_address(from_addr, pool.as_ref())
                .await
                .map_err(crate::ServiceError::Database)?
        {
            // 同步签名的状态
            MultisigQueueRepo::sync_sign_status(
                id,
                account_id,
                multisig_account.threshold,
                _res.status,
                pool,
            )
            .await?;

            // 系统通知
            let notification = Notification::new_confirmation_notification(
                &multisig_account.name,
                &multisig_account.address,
                &multisig_account.id,
                transfer_type,
                NotificationType::Confirmation,
            );
            let system_notification_service = SystemNotificationService::new(repo);
            system_notification_service
                .add_system_notification(id, notification, 0)
                .await?;
        };

        let data = crate::notify::NotifyEvent::MultiSignTransAccept(
            crate::notify::event::transaction::MultiSignTransAcceptFrontend {
                id: id.to_string(),
                from_addr: from_addr.to_string(),
                to_addr: to_addr.to_string(),
                value: value.to_string(),
                expiration,
                symbol: symbol.to_string(),
                chain_code: chain_code.to_string(),
                token_addr: token_addr.clone(),
                msg_hash: msg_hash.to_string(),
                tx_hash: tx_hash.to_string(),
                raw_data: raw_data.to_string(),
                status,
                notes: notes.to_string(),
                bill_kind: transfer_type,
                created_at,
            },
        );
        crate::notify::FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        mqtt::payload::incoming::transaction::MultiSignTransAccept, test::env::get_manager,
    };

    #[tokio::test]
    async fn acct_change() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        let str1 = r#"{"id":"220035849155383296","fromAddr":"TD7MfEmAiFxhtdeuCia69k1G3wco5fmmS2","toAddr":"TVeGPYyJ8DFWPRxmBSUCKgK63TkYxboVBM","value":"3.33333333","expiration":1737426534,"symbol":"TRX","chainCode":"tron","tokenAddr":null,"msgHash":"e1558c6a72a2bc444d5069380fdd433d31d8764b7b8becfdbc5ba9573974e4f7","txHash":"","rawData":"QAAAAAAAAABlMTU1OGM2YTcyYTJiYzQ0NGQ1MDY5MzgwZmRkNDMzZDMxZDg3NjRiN2I4YmVjZmRiYzViYTk1NzM5NzRlNGY3dwEAAAAAAAB7ImNvbnRyYWN0IjpbeyJwYXJhbWV0ZXIiOnsidmFsdWUiOnsiYW1vdW50IjozMzMzMzMzLCJvd25lcl9hZGRyZXNzIjoiNDEyMjcyZWViZWIwNjhkOGE1ODk2MTY4ZjhkZjc1ZDA0NjczZDgxOWQxIiwidG9fYWRkcmVzcyI6IjQxZDdjZDcwYmU0YzNhNTExZDAwMDcwYzA4NGY5Y2ViMmNjZWMyOWU4MSJ9LCJ0eXBlX3VybCI6InR5cGUuZ29vZ2xlYXBpcy5jb20vcHJvdG9jb2wuVHJhbnNmZXJDb250cmFjdCJ9LCJ0eXBlIjoiVHJhbnNmZXJDb250cmFjdCJ9XSwicmVmX2Jsb2NrX2J5dGVzIjoiN2UwNiIsInJlZl9ibG9ja19oYXNoIjoiZjU3OWI4MDNjZmZjNDY2ZSIsImV4cGlyYXRpb24iOjE3Mzc0MjY1OTEwMDAsInRpbWVzdGFtcCI6MTczNzM4MzMzNDE3NX0MAQAAAAAAADBhMDI3ZTA2MjIwOGY1NzliODAzY2ZmYzQ2NmU0MDk4YmFjN2I1YzgzMjVhNjgwODAxMTI2NDBhMmQ3NDc5NzA2NTJlNjc2ZjZmNjc2YzY1NjE3MDY5NzMyZTYzNmY2ZDJmNzA3MjZmNzQ2ZjYzNmY2YzJlNTQ3MjYxNmU3MzY2NjU3MjQzNmY2ZTc0NzI2MTYzNzQxMjMzMGExNTQxMjI3MmVlYmViMDY4ZDhhNTg5NjE2OGY4ZGY3NWQwNDY3M2Q4MTlkMTEyMTU0MWQ3Y2Q3MGJlNGMzYTUxMWQwMDA3MGMwODRmOWNlYjJjY2VjMjllODExOGQ1YjljYjAxNzA5ZmEyZjdhMGM4MzIAAAAAAAAAAA==","status":1,"notes":"","createdAt":"2025-01-20T14:28:54Z","signatures":[{"queue_id":"220035849155383296","address":"TD7MfEmAiFxhtdeuCia69k1G3wco5fmmS2","signature":"936ac873a9844f33c350204feedbb1ea6f541799e3671fb2550790faf8dac6f50a95c1d08c150913a3318f8ffc015a93da936279d8ac26d53c9d4a1a324f58d800","status":1}],"accountId":"219950770760585216","transferType":1}"#;
        let changet = serde_json::from_str::<MultiSignTransAccept>(&str1).unwrap();

        let res = changet.exec("1").await;
        println!("{:?}", res);
        Ok(())
    }
}
