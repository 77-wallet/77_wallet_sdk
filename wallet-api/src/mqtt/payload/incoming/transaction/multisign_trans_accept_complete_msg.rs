use wallet_database::{
    entities::multisig_signatures::{MultisigSignatureStatus, NewSignatureEntity},
    repositories::multisig_queue::MultisigQueueRepo,
};

// 签名的结果同步给所有人
use super::{MultiSignTransAcceptCompleteMsg, MultiSignTransAcceptCompleteMsgBody};

impl MultiSignTransAcceptCompleteMsg {
    pub(crate) async fn exec(self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let event_name = self.name();
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting MultiSignTransAcceptCompleteMsg processing");
        let MultiSignTransAcceptCompleteMsg(body) = &self;

        for msg in body {
            let params: NewSignatureEntity = msg.try_into()?;

            MultisigQueueRepo::create_or_update_sign(&params, &pool).await?;

            let data = crate::notify::NotifyEvent::MultiSignTransAcceptCompleteMsg(msg.to_owned());
            crate::notify::FrontendNotifyEvent::new(data).send().await?;
        }

        // sync sign status
        if let Some(item) = body.first() {
            // if MultisigAccountDaoV1::find_by_address(&item.address, pool.as_ref())
            //     .await
            //     .map_err(crate::ServiceError::Database)?
            //     .is_none()
            // {
            //     let mut repo = RepositoryFactory::repo(pool.clone());
            //     MultisigDomain::recover_multisig_data_by_address(&mut repo, &item.address).await?;
            // }

            // let account =
            //     MultisigQueueDaoV1::find_by_id_with_account(&item.queue_id, pool.as_ref())
            //         .await
            //         .map_err(|e| crate::ServiceError::Database(e.into()))?;

            let queue = MultisigQueueRepo::find_by_id(&pool, &item.queue_id).await?;
            if let Some(queue) = queue {
                MultisigQueueRepo::sync_sign_status(&queue, queue.status, pool.clone()).await?;
            }
        }

        Ok(())
    }
}

impl TryFrom<&MultiSignTransAcceptCompleteMsgBody> for NewSignatureEntity {
    fn try_from(value: &MultiSignTransAcceptCompleteMsgBody) -> Result<Self, Self::Error> {
        let status: MultisigSignatureStatus = (value.status as i32).try_into()?;
        Ok(Self {
            queue_id: value.queue_id.to_string(),
            address: value.address.to_string(),
            signature: value.signature.to_string(),
            status,
            weight: None,
        })
    }

    type Error = crate::ServiceError;
}

#[cfg(test)]
mod test {
    use crate::{
        mqtt::payload::incoming::transaction::MultiSignTransAcceptCompleteMsg,
        test::env::get_manager,
    };

    #[tokio::test]
    async fn test_() {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>

        let raw = r#"
        [
                {
                    "address": "TYD6wPezLZKAqHEH5SexzX9kAocYibB3er",
                    "queueId": "243784320156831744",
                    "signature": "88fb989a0fa51ba6aef4f5861317b19ec36e8c097c9a364eed14186e455a5205775a3e4591347516a08d6e8ab7051d52a788108fd951e352a0229e4fd3ea89e300",
                    "status": 1
                }
            ]
        "#;
        let res = serde_json::from_str::<MultiSignTransAcceptCompleteMsg>(&raw).unwrap();

        let (_, _) = get_manager().await.unwrap();
        let mut handles = Vec::new();
        for _i in 0..4 {
            let item = res.clone();
            let c = tokio::spawn(async { item.exec("x").await.unwrap() });
            handles.push(c);
        }

        for handle in handles {
            // handle.await 返回一个 Result<T, JoinError>
            handle.await.unwrap();
        }
    }
}
