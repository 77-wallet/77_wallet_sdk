use wallet_database::{
    entities::multisig_signatures::{MultisigSignatureStatus, NewSignatureEntity},
    repositories::multisig_queue::MultisigQueueRepo,
};

use crate::{
    domain::multisig::MultisigQueueDomain,
    messaging::notify::{event::NotifyEvent, FrontendNotifyEvent},
};

//  多签交易签名同步给其他成员
// biz_type = MULTI_SIGN_TRANS_ACCEPT_COMPLETE_MSG
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultiSignTransAcceptCompleteMsg(Vec<MultiSignTransAcceptCompleteMsgBody>);

impl MultiSignTransAcceptCompleteMsg {
    pub(crate) fn name(&self) -> String {
        "MULTI_SIGN_TRANS_ACCEPT_COMPLETE_MSG".to_string()
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultiSignTransAcceptCompleteMsgBody {
    pub queue_id: String,
    pub address: String,
    pub signature: String,
    /// 0未签 1签名  2拒绝
    pub status: i8,
}

impl From<&NewSignatureEntity> for MultiSignTransAcceptCompleteMsgBody {
    fn from(value: &NewSignatureEntity) -> Self {
        Self {
            queue_id: value.queue_id.clone(),
            address: value.address.clone(),
            signature: value.signature.clone(),
            status: value.status.to_i8(),
        }
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

// 签名的结果同步给所有人
impl MultiSignTransAcceptCompleteMsg {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
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

            // let data = NotifyEvent::MultiSignTransAcceptCompleteMsg(msg.to_owned());
            // FrontendNotifyEvent::new(data).send().await?;
        }

        // sync sign status
        if let Some(item) = body.first() {
            let queue = MultisigQueueRepo::find_by_id(&pool, &item.queue_id).await?;
            if let Some(queue) = queue {
                MultisigQueueRepo::sync_sign_status(&queue, queue.status, pool.clone()).await?;

                MultisigQueueDomain::update_raw_data(&queue.id, pool.clone()).await?;
            }
        }

        // 最后同步消息给到前端。
        for msg in body {
            let data = NotifyEvent::MultiSignTransAcceptCompleteMsg(msg.to_owned());
            FrontendNotifyEvent::new(data).send().await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{messaging::mqtt::topics::MultiSignTransAcceptCompleteMsg, test::env::get_manager};

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
