use std::sync::Arc;

use wallet_database::entities::task_queue::TaskQueueEntity;

use crate::{domain::app::mqtt::MqttDomain, FrontendNotifyEvent};

pub async fn process_unconfirm_msg(
    client_id: &str,
    pool: wallet_database::DbPool,
    notify: Arc<tokio::sync::Notify>,
) -> Result<(), crate::ServiceError> {
    let client_id = client_id.to_string();
    tokio::spawn(async move {
        tracing::info!("process_unconfirm_msg start");
        loop {
            tokio::select! {
                _ = notify.notified() => {
                    tracing::debug!("收到通知，开始处理");
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                    tracing::debug!("30秒超时，开始自动处理");
                }
            }

            if let Err(e) = do_process_unconfirm_msg(&client_id, pool.clone()).await {
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
            // match timeout(std::time::Duration::from_secs(30), rx.changed()).await {
            //     Ok(Ok(_)) => {
            //         tracing::info!("收到通知，继续执行未确认消息任务");
            //         if let Err(e) = do_process_unconfirm_msg(&client_id, pool.clone()).await {
            //             tracing::error!("处理未确认消息失败: {}", e);
            //             // 尝试发送错误通知给前端
            //             if let Err(send_err) = FrontendNotifyEvent::send_error(
            //                 "InitializationTask::ProcessUnconfirmMsg",
            //                 e.to_string(),
            //             )
            //             .await
            //             {
            //                 tracing::error!("发送错误通知失败: {}", send_err);
            //             }
            //         }
            //     }
            //     Ok(Err(_)) => {
            //         tracing::warn!("通知通道已关闭，退出监听任务");
            //         break; // 通道关闭，退出循环
            //     }
            //     Err(_) => {
            //         tracing::warn!("等待通知超时，继续等待下一次通知");
            //         // 超时继续等待，不break，保持循环
            //     }
            // }
        }
    });

    Ok(())
}

async fn do_process_unconfirm_msg(
    client_id: &str,
    pool: wallet_database::DbPool,
) -> Result<(), crate::ServiceError> {
    match TaskQueueEntity::has_unfinished_task(&*pool).await {
        Ok(true) => {
            tracing::debug!("存在未完成任务，跳过处理未确认消息");
            return Ok(());
        }
        Ok(false) => {
            tracing::debug!("不存在未完成任务，处理未确认消息");
        }
        Err(e) => {
            tracing::error!("has_unfinished_task error: {}", e);
            return Err(e.into());
        }
    }

    if let Err(e) = MqttDomain::process_unconfirm_msg(&client_id).await {
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
