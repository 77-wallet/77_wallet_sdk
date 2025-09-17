use crate::messaging::notify::{
    FrontendNotifyEvent, event::NotifyEvent, transaction::ConfirmationFrontend,
};
use wallet_database::{
    entities::{
        bill::BillKind,
        multisig_queue::{MultisigQueueEntity, MultisigQueueStatus, NewMultisigQueueEntity},
        multisig_signatures::{
            MultisigSignatureEntity, MultisigSignatureStatus, NewSignatureEntity,
        },
    },
    repositories::multisig_queue::MultisigQueueRepo,
};

// 创建多签交易同步给其他的参与方
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultiSignTransAccept {
    pub queue: MultisigQueueEntity,
    pub signatures: Vec<MultisigSignatureEntity>,
}
impl MultiSignTransAccept {
    pub(crate) fn name(&self) -> String {
        "MULTI_SIGN_TRANS_ACCEPT".to_string()
    }
}

impl TryFrom<&MultiSignTransAccept> for NewMultisigQueueEntity {
    type Error = crate::error::service::ServiceError;
    fn try_from(value: &MultiSignTransAccept) -> Result<Self, crate::error::service::ServiceError> {
        let signatures = value
            .signatures
            .iter()
            .map(|s| NewSignatureEntity {
                queue_id: value.queue.id.to_string(),
                address: s.address.to_string(),
                signature: s.signature.to_string(),
                status: MultisigSignatureStatus::try_from(s.status as i32).unwrap(),
                weight: None,
            })
            .collect::<Vec<_>>();

        Ok(Self {
            id: value.queue.id.to_string(),
            account_id: value.queue.account_id.to_string(),
            from_addr: value.queue.from_addr.to_string(),
            to_addr: value.queue.to_addr.to_string(),
            value: value.queue.value.to_string(),
            symbol: value.queue.symbol.to_string(),
            expiration: value.queue.expiration,
            chain_code: value.queue.chain_code.to_string(),
            token_addr: value.queue.token_addr.clone(),
            msg_hash: value.queue.msg_hash.to_string(),
            tx_hash: value.queue.tx_hash.to_string(),
            raw_data: value.queue.raw_data.to_string(),
            status: MultisigQueueStatus::from_i8(value.queue.status),
            notes: value.queue.notes.to_string(),
            fail_reason: value.queue.fail_reason.to_string(),
            signatures,
            create_at: value.queue.created_at,
            transfer_type: BillKind::try_from(value.queue.transfer_type).unwrap(),
            permission_id: value.queue.permission_id.to_string(),
        })
    }
}

impl MultiSignTransAccept {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let event_name = self.name();
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting MultiSignTransAccept processing");

        // 新增交易队列数据
        let mut params = NewMultisigQueueEntity::try_from(self)?;
        let queue = MultisigQueueRepo::create_queue_with_sign(pool.clone(), &mut params).await?;

        // 同步签名的状态
        MultisigQueueRepo::sync_sign_status(&queue, queue.status, pool.clone()).await?;

        // self.system_notify(&queue, pool).await?;

        let data = NotifyEvent::Confirmation(ConfirmationFrontend::try_from(self)?);
        FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{messaging::mqtt::topics::MultiSignTransAccept, test::env::get_manager};

    #[tokio::test]
    async fn acct_change() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        let str1 = r#"{"queue":{"id":"236618098902437888","from_addr":"TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ","to_addr":"TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR","value":"1","expiration":1741344050,"symbol":"TRX","chain_code":"tron","token_addr":null,"msg_hash":"2229df18911ebfe143bfd129a9a083364ab3de63da01887ff535cf0b45685415","tx_hash":"","raw_data":"QAAAAAAAAAAyMjI5ZGYxODkxMWViZmUxNDNiZmQxMjlhOWEwODMzNjRhYjNkZTYzZGEwMTg4N2ZmNTM1Y2YwYjQ1Njg1NDE1dwEAAAAAAAB7ImNvbnRyYWN0IjpbeyJwYXJhbWV0ZXIiOnsidmFsdWUiOnsiYW1vdW50IjoxMDAwMDAwLCJvd25lcl9hZGRyZXNzIjoiNDE4ODM3YzhkNzJhNDUwNTRjNmVjZDJlZDU5NmU4YzNiMDIzZTc3ZWUzIiwidG9fYWRkcmVzcyI6IjQxY2NjYTgzODIwMzY1MjE2NjY0OTFkZDk0ODQyODJkNjUzNjNhZTcwZiJ9LCJ0eXBlX3VybCI6InR5cGUuZ29vZ2xlYXBpcy5jb20vcHJvdG9jb2wuVHJhbnNmZXJDb250cmFjdCJ9LCJ0eXBlIjoiVHJhbnNmZXJDb250cmFjdCJ9XSwicmVmX2Jsb2NrX2J5dGVzIjoiMGVkNiIsInJlZl9ibG9ja19oYXNoIjoiMmQ0NjM0YzM3N2UzZWMwZCIsImV4cGlyYXRpb24iOjE3NDEzNDQxMDgwMDAsInRpbWVzdGFtcCI6MTc0MTMzNjg1MDcyMn0KAQAAAAAAADBhMDIwZWQ2MjIwODJkNDYzNGMzNzdlM2VjMGQ0MGUwZGJjOTgxZDczMjVhNjcwODAxMTI2MzBhMmQ3NDc5NzA2NTJlNjc2ZjZmNjc2YzY1NjE3MDY5NzMyZTYzNmY2ZDJmNzA3MjZmNzQ2ZjYzNmY2YzJlNTQ3MjYxNmU3MzY2NjU3MjQzNmY2ZTc0NzI2MTYzNzQxMjMyMGExNTQxODgzN2M4ZDcyYTQ1MDU0YzZlY2QyZWQ1OTZlOGMzYjAyM2U3N2VlMzEyMTU0MWNjY2E4MzgyMDM2NTIxNjY2NDkxZGQ5NDg0MjgyZDY1MzYzYWU3MGYxOGMwODQzZDcwYTJlMjhlZmVkNjMyAAAAAAAAAAA=","status":2,"notes":"salary","fail_reason":"","created_at":"2025-03-07T08:40:50Z","updated_at":null,"account_id":"195330590956982272","transfer_type":1,"permission_id":""},"signatures":[{"id":19,"queue_id":"236618098902437888","address":"TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ","signature":"6aeb7f11e5155bdca868af1da8cff62101ba8687bcf6da68817bc332d55f750713f29998c93af69f60d49905c295f67b53a4316045816d8d890065b0684d986600","status":1,"created_at":"2025-03-07T08:40:52Z","updated_at":null},{"id":20,"queue_id":"236618098902437888","address":"TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1","signature":"4cef26991bc0f536900be1a741e7dc72b29623f8f7a57a94e1abfe52ae4cc11a66c953db37d97c52608b48ff6804ea8e570847fca1011ec2faabb2236b2e544501","status":1,"created_at":"2025-03-07T08:40:52Z","updated_at":null}]}"#;
        let change = serde_json::from_str::<MultiSignTransAccept>(&str1).unwrap();

        let res = change.exec("1").await;
        println!("{:?}", res);
        Ok(())
    }
}
