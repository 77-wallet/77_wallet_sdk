use wallet_database::{
    dao::multisig_queue::MultisigQueueDaoV1, entities::multisig_queue::fail_reason,
};

// 取消多签交易
use super::MultiSignTransCancel;

impl MultiSignTransCancel {
    pub async fn exec(self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let event_name = self.name();
        let pool = crate::Context::get_global_sqlite_pool()?;
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting MultiSignTransCancel processing");
        // 并发可能导致查询不出来结果
        let res = MultisigQueueDaoV1::find_by_id(&self.withdraw_id, pool.as_ref())
            .await
            .map_err(|e| crate::ServiceError::Database(e.into()))?;
        if res.is_none() {
            tracing::error!(
                event_name = %event_name,
                "Cancel multisig queue faild affetd :0");
            return Err(crate::ServiceError::Business(
                crate::MultisigQueueError::NotFound.into(),
            ));
        }

        MultisigQueueDaoV1::update_fail(&self.withdraw_id, fail_reason::CANCEL, pool.as_ref())
            .await
            .map_err(|e| crate::ServiceError::Database(e.into()))?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        mqtt::payload::incoming::transaction::MultiSignTransCancel, test::env::get_manager,
    };

    #[tokio::test]
    async fn acct_change() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        let str1 = r#"{"withdrawId":"220035849155383296"}"#;
        let changet = serde_json::from_str::<MultiSignTransCancel>(&str1).unwrap();

        let res = changet.exec("1").await;
        println!("{:?}", res);
        Ok(())
    }
}
