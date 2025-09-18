use crate::messaging::notify::{FrontendNotifyEvent, event::NotifyEvent};
use wallet_database::{
    entities::multisig_queue::fail_reason, repositories::multisig_queue::MultisigQueueRepo,
};

// 取消多签交易
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultiSignTransCancel {
    pub withdraw_id: String,
}

impl MultiSignTransCancel {
    pub(crate) fn name(&self) -> String {
        "MULTI_SIGN_TRANS_CANCEL".to_string()
    }
}

impl MultiSignTransCancel {
    pub async fn exec(&self, _msg_id: &str) -> Result<(), crate::error::service::ServiceError> {
        let event_name = self.name();
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting MultiSignTransCancel processing");

        // 并发可能导致查询不出来结果(事件的先后顺序不一致，导致错误)
        let queue = MultisigQueueRepo::find_by_id(&pool, &self.withdraw_id).await?;
        if queue.is_none() {
            tracing::error!(
                event_name = %event_name,
                "Cancel multisig queue faild affetd :0");
            return Err(crate::error::service::ServiceError::Business(
                crate::error::business::multisig_queue::MultisigQueueError::NotFound.into(),
            ));
        }

        MultisigQueueRepo::update_fail(&pool, &self.withdraw_id, fail_reason::CANCEL).await?;

        let data = NotifyEvent::RecoverComplete;
        FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{messaging::mqtt::topics::MultiSignTransCancel, test::env::get_manager};

    #[tokio::test]
    async fn acct_change() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        let str1 = r#"{"withdrawId":"236618098902437888"}"#;
        let changet = serde_json::from_str::<MultiSignTransCancel>(&str1).unwrap();

        let res = changet.exec("1").await;
        println!("{:?}", res);
        Ok(())
    }
}
