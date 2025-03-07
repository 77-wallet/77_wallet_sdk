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
                    "address": "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1",
                    "queueId": "236631132983136256",
                    "signature": "74eb8147bb31e4f74c9a25f9fd8498bbf28326cf4e6d0cb804268fb6214181931995702713373bb2ae07ebb25728d9ec3c14b36807ab99e8ecf8711d7ab848ba01",
                    "status": 1
                },
                {
                    "address": "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR",
                    "queueId": "236631132983136256",
                    "signature": "",
                    "status": 0
                }
            ]
        "#;
        let res = serde_json::from_str::<MultiSignTransAcceptCompleteMsg>(&raw).unwrap();

        let (_, _) = get_manager().await.unwrap();

        res.exec("x").await.unwrap();
    }
}
