use crate::infrastructure::task_queue::{BackendApiTask, BackendApiTaskData, Task, Tasks};
use wallet_transport_backend::request::SendMsgConfirm;

pub(crate) struct TaskQueueDomain;

impl TaskQueueDomain {
    pub async fn send_msg_confirm(ids: Vec<SendMsgConfirm>) -> Result<(), crate::ServiceError> {
        if !ids.is_empty() {
            const BATCH_SIZE: usize = 500;
            for chunk in ids.chunks(BATCH_SIZE) {
                let api = crate::Context::get_global_backend_api()?;
                let aes_cbc_cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
                // tracing::info!("send_msg_confirm: {}", chunk.len());
                api.send_msg_confirm(
                    aes_cbc_cryptor,
                    &wallet_transport_backend::request::SendMsgConfirmReq::new(chunk.to_vec()),
                )
                .await?;
            }
        }
        Ok(())
    }

    // send a request to backend if failed wrap to task
    pub async fn send_or_wrap_task<T: serde::Serialize + std::fmt::Debug>(
        req: T,
        endpoint: &str,
    ) -> Result<Option<Task>, crate::ServiceError> {
        let backend = crate::Context::get_global_backend_api()?;
        let aes_cbc_cryptor = crate::manager::Context::get_global_aes_cbc_cryptor()?;

        let res = backend
            .post_request::<_, serde_json::Value>(endpoint, &req, aes_cbc_cryptor)
            .await;

        if let Err(e) = res {
            tracing::error!("request backend:{},req:{:?} error:{}", endpoint, req, e);

            let task = Task::BackendApi(BackendApiTask::BackendApi(BackendApiTaskData::new(
                endpoint, &req,
            )?));
            return Ok(Some(task));
        }
        Ok(None)
    }

    // 发送任务,如果失败放入到队列中去
    pub async fn send_or_to_queue<T: serde::Serialize + std::fmt::Debug>(
        req: T,
        endpoint: &str,
    ) -> Result<(), crate::ServiceError> {
        let task = Self::send_or_wrap_task(req, endpoint).await?;

        if let Some(task) = task {
            Tasks::new().push(task).send().await?;
        }

        Ok(())
    }

    // pub async fn get_task_queue_status() -> Result<(), crate::ServiceError> {
    //     let pool = crate::manager::Context::get_global_sqlite_pool()?;
    //     let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
    //     use wallet_database::repositories::task_queue::TaskQueueRepoTrait as _;
    //     let task_queue = repo.task_running(&repo, 0).await?;
    //     Ok(())
    // }
}
