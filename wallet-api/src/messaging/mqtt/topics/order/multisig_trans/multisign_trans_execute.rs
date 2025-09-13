use wallet_database::{
    entities::multisig_queue::MultisigQueueStatus, repositories::multisig_queue::MultisigQueueRepo,
};

use crate::{FrontendNotifyEvent, NotifyEvent};

// 取消多签交易
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultiSignTransExecute {
    pub withdraw_id: String,
}

impl MultiSignTransExecute {
    pub(crate) fn _name(&self) -> String {
        "MULTI_SIGN_TRANS_EXECUTE".to_string()
    }
}

// 当某一个参与放执行交易后同步其他参与放交易的状态
impl MultiSignTransExecute {
    pub async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 并发可能导致查询不出来结果(事件的先后顺序不一致，导致错误)
        MultisigQueueRepo::update_status(
            &self.withdraw_id,
            MultisigQueueStatus::InConfirmation,
            &pool,
        )
        .await?;

        // 发送一个事件去让前端更新全局消息
        let data = NotifyEvent::MultiSignTransExecute;
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }
}
