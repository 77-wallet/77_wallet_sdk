use crate::{FrontendNotifyEvent, domain::app::mqtt::MqttDomain};
use std::{collections::HashSet, sync::Arc};
use tokio::time::Instant;
use wallet_database::repositories::task_queue::TaskQueueRepoTrait;

#[derive(Debug, Clone)]
pub(crate) struct UnconfirmedMsgCollector {
    tx: tokio::sync::mpsc::UnboundedSender<Vec<String>>,
}

impl UnconfirmedMsgCollector {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        Self::start_collect(rx);
        Self { tx }
    }

    pub fn submit(&self, ids: Vec<String>) -> Result<(), crate::ServiceError> {
        self.tx
            .send(ids)
            .map_err(|e| crate::SystemError::ChannelSendFailed(e.to_string()))?;
        Ok(())
    }

    pub fn start_collect(mut rx: tokio::sync::mpsc::UnboundedReceiver<Vec<String>>) {
        tokio::spawn(async move {
            let mut buffer = HashSet::new();
            let mut last_recv_time: Option<Instant> = None;

            loop {
                tokio::select! {
                    Some(ids) = rx.recv() => {
                        for id in ids {
                            buffer.insert(id);
                        }
                        if last_recv_time.is_none() {
                            last_recv_time = Some(Instant::now());
                        }
                    }
                    _ = async {
                        if let Some(start) = last_recv_time {
                            let elapsed = start.elapsed();
                            if elapsed < tokio::time::Duration::from_secs(5) {
                                tokio::time::sleep(tokio::time::Duration::from_secs(5) - elapsed).await;
                            }
                        } else {
                            // 初始 sleep，避免 busy loop
                            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        }
                    } => {
                        if !buffer.is_empty() {
                            let confirm_ids: Vec<_> = buffer.drain().collect();
                            tracing::debug!("批量确认消息: {:?}", confirm_ids.len());

                            let confirms = confirm_ids
                                .iter()
                                .map(|id| {
                                    wallet_transport_backend::request::SendMsgConfirm::new(
                                        id,
                                        wallet_transport_backend::request::MsgConfirmSource::Other,
                                    )
                                })
                                .collect::<Vec<_>>();

                            if let Err(e) = crate::domain::task_queue::TaskQueueDomain::send_msg_confirm(confirms).await {
                                tracing::error!("发送确认失败: {:?}", e);
                            }

                            last_recv_time = None;

                            let notify = crate::manager::Context::get_global_notify().unwrap();
                            notify.notify_one();
                            tracing::debug!("notify_one");
                        }else{
                            tracing::debug!("⏳ 等待消息确认");
                        }
                    }

                }
            }
        });
    }
}

#[derive(Debug, Clone)]
pub struct UnconfirmedMsgProcessor {
    client_id: String,
    notify: Arc<tokio::sync::Notify>,
}

impl UnconfirmedMsgProcessor {
    pub fn new(client_id: &str, notify: Arc<tokio::sync::Notify>) -> Self {
        Self {
            client_id: client_id.into(),
            notify,
        }
    }

    async fn handle_once(client_id: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;

        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        let failed_tasks = repo.failed_mqtt_task_queue().await?;

        if failed_tasks.len() < 500 {
            tracing::debug!("未完成的mqtt任务数小于500个，处理未确认消息");
        } else {
            tracing::debug!("未完成的mqtt任务达到500个，跳过处理未确认消息");
            return Ok(());
        }
        // match TaskQueueEntity::has_unfinished_task_by_type(&*pool, 2).await {
        //     Ok(true) => {
        //         tracing::debug!("存在未完成的mqtt任务，跳过处理未确认消息");
        //         return Ok(());
        //     }
        //     Ok(false) => {
        //         tracing::debug!("不存在未完成mqtt任务，处理未确认消息");
        //     }
        //     Err(e) => {
        //         tracing::error!("has_unfinished_task error: {}", e);
        //         return Err(e.into());
        //     }
        // }

        if let Err(e) = MqttDomain::process_unconfirm_msg(client_id).await {
            if let Err(e) = FrontendNotifyEvent::send_error(
                "InitializationTask::ProcessUnconfirmMsg",
                e.to_string(),
            )
            .await
            {
                tracing::error!("send_error error: {}", e);
            }
            tracing::error!("process unconfirm msg error:{}", e);
        };

        Ok(())
    }

    pub async fn start(&self) -> Result<(), crate::ServiceError> {
        let client_id = self.client_id.to_string();
        let notify = self.notify.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::handle_once(&client_id).await {
                if let Err(send_err) = FrontendNotifyEvent::send_error(
                    "InitializationTask::ProcessUnconfirmMsg",
                    e.to_string(),
                )
                .await
                {
                    tracing::error!("send_error error: {}", send_err);
                }
            };
            tracing::debug!("process_unconfirm_msg start");
            loop {
                tokio::select! {
                    _ = notify.notified() => {
                        tracing::debug!("收到通知，开始处理");
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                        tracing::debug!("30秒超时，开始自动处理");
                    }
                }
                if let Err(e) = Self::handle_once(&client_id).await {
                    tracing::error!("处理未确认消息失败: {}", e);
                    // 尝试发送错误通知给前端
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
        });

        Ok(())
    }
}
