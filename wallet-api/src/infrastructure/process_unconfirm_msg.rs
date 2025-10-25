use crate::{
    context::CONTEXT, domain::app::mqtt::MqttDomain, error::service::ServiceError,
    messaging::notify::FrontendNotifyEvent,
};
use std::sync::Arc;
use tokio::{
    sync::{Mutex, broadcast},
    task::JoinHandle,
};
use wallet_database::repositories::task_queue::TaskQueueRepoTrait;
use wallet_transport_backend::request::api_wallet::msg::MsgAckExpiredResendReq;

#[derive(Debug)]
pub struct UnconfirmedMsgProcessorHandle {
    shutdown_tx: broadcast::Sender<()>,
    handle: Mutex<Option<JoinHandle<Result<(), ServiceError>>>>,
}

impl UnconfirmedMsgProcessorHandle {
    pub async fn new(client_id: &str, notify: Arc<tokio::sync::Notify>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let shutdown_rx1 = shutdown_tx.subscribe();
        // 发交易
        let mut tx = UnconfirmedMsgProcessor::new(client_id, notify);
        let tx_handle = tokio::spawn(async move { tx.start().await });
        Self { shutdown_tx, handle: Mutex::new(Some(tx_handle)) }
    }

    pub(crate) async fn close(&self) -> Result<(), ServiceError> {
        let _ = self.shutdown_tx.send(());
        if let Some(handle) = self.handle.lock().await.take() {
            handle.await.map_err(|_| {
                ServiceError::System(crate::error::system::SystemError::BackendEndpointNotFound)
            })??;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct UnconfirmedMsgProcessor {
    client_id: String,
    notify: Arc<tokio::sync::Notify>,
}

impl UnconfirmedMsgProcessor {
    pub fn new(client_id: &str, notify: Arc<tokio::sync::Notify>) -> Self {
        Self { client_id: client_id.into(), notify }
    }

    async fn handle_once(&self) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 判断数据库中是否存在大量的未处理消息,如果有则跳过
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        if repo.failed_mqtt_task_queue().await?.len() < 500 {
            tracing::debug!("未完成的mqtt任务数小于500个,处理未确认消息");
        } else {
            tracing::debug!("未完成的mqtt任务达到500个,跳过处理未确认消息");
            return Ok(());
        }

        MqttDomain::process_unconfirm_msg(&self.client_id).await
    }

    async fn handle_and_report(&self) {
        if let Err(e) = self.handle_once().await {
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

    async fn api_wallet_msg_resend(&self) {
        let ctx = CONTEXT.get().unwrap();
        let backend = ctx.get_global_backend_api();
        let res = backend
            .msg_ack_expired_resend(MsgAckExpiredResendReq {
                client_id: self.client_id.to_string(),
            })
            .await;
        match res {
            Ok(_) => {}
            Err(e) => {
                tracing::error!(" ---- {}", e)
            }
        }
    }

    /// Runs once at startup, then repeats either when notified
    /// or every 30 seconds on a timer.
    pub async fn start(&self) -> Result<(), ServiceError> {
        let client_id = self.client_id.to_string();
        let notify = self.notify.clone();
        let mut interval_30sec = tokio::time::interval(std::time::Duration::from_secs(30));
        let mut interval_10min = tokio::time::interval(std::time::Duration::from_secs(60 * 10));

        // 启动的时候执行一次
        self.handle_and_report().await;
        self.api_wallet_msg_resend().await;
        loop {
            tokio::select! {
                 _ = notify.notified() => {
                     tracing::debug!("收到通知，开始处理");
                    // 定时执行
                    self.handle_and_report().await;
                 }
                 _ = interval_30sec.tick() => {
                     tracing::debug!("30秒超时,开始自动处理");
                    // 定时执行
                    self.handle_and_report().await;
                 }
                _ = interval_10min.tick() => {
                    self.api_wallet_msg_resend().await;
                }
            }
        }
        Ok(())
    }
}
