use crate::{
    domain::app::mqtt::MqttDomain, error::service::ServiceError,
    messaging::notify::FrontendNotifyEvent,
};
use std::{collections::HashSet, sync::Arc};
use tokio::time::Instant;
use wallet_database::repositories::task_queue::TaskQueueRepoTrait;

#[derive(Debug, Clone)]
pub struct UnconfirmedMsgProcessor {
    client_id: String,
    notify: Arc<tokio::sync::Notify>,
}

impl UnconfirmedMsgProcessor {
    pub fn new(client_id: &str, notify: Arc<tokio::sync::Notify>) -> Self {
        Self { client_id: client_id.into(), notify }
    }

    async fn handle_once(client_id: &str) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 判断数据库中是否存在大量的未处理消息,如果有则跳过
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        if repo.failed_mqtt_task_queue().await?.len() < 500 {
            tracing::debug!("未完成的mqtt任务数小于500个,处理未确认消息");
        } else {
            tracing::debug!("未完成的mqtt任务达到500个,跳过处理未确认消息");
            return Ok(());
        }

        MqttDomain::process_unconfirm_msg(client_id).await
    }

    async fn handle_and_report(client_id: &str) {
        if let Err(e) = Self::handle_once(client_id).await {
            tracing::error!("处理未确认消息失败: {}", e);
            if let Err(send_err) = FrontendNotifyEvent::send_error(
                "InitializationTask::ProcessUnconfirmMsg",
                e.to_string(),
            )
            .await
            {
                tracing::error!("发送错误通知失败: {}", send_err);
            }
        }
    }

    /// Runs once at startup, then repeats either when notified
    /// or every 30 seconds on a timer.
    pub async fn start(&self) {
        let client_id = self.client_id.to_string();
        let notify = self.notify.clone();
        tokio::spawn(async move {
            // 启动的时候执行一次
            Self::handle_and_report(&client_id).await;
            loop {
                tokio::select! {
                     _ = notify.notified() => {
                         tracing::debug!("收到通知，开始处理");
                     }
                     _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                         tracing::debug!("30秒超时,开始自动处理");
                     }
                }
                // 定时执行
                Self::handle_and_report(&client_id).await;
            }
        });
    }
}
