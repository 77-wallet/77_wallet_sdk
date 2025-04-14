use wallet_database::entities::task_queue::TaskName;

use super::task_handle::backend_handle::BackendTaskHandle;

pub(crate) enum BackendApiTask {
    BackendApi(BackendApiTaskData),
}

impl BackendApiTask {
    pub(crate) fn get_name(&self) -> TaskName {
        TaskName::BackendApi
    }

    pub(crate) fn get_body(&self) -> Result<Option<String>, crate::ServiceError> {
        match self {
            BackendApiTask::BackendApi(api_data) => {
                Ok(Some(wallet_utils::serde_func::serde_to_string(api_data)?))
            }
        }
    }
}

impl BackendApiTask {
    pub fn new<T>(endpoint: &str, body: &T) -> Result<Self, crate::ServiceError>
    where
        T: serde::Serialize,
    {
        Ok(Self::BackendApi(BackendApiTaskData::new(endpoint, body)?))
    }
}

// 所有请求后端的task，公用结构
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct BackendApiTaskData {
    pub endpoint: String,
    pub body: serde_json::Value,
}

impl BackendApiTaskData {
    pub(crate) fn new<T>(endpoint: &str, body: &T) -> Result<Self, crate::ServiceError>
    where
        T: serde::Serialize,
    {
        Ok(Self {
            endpoint: endpoint.to_string(),
            body: wallet_utils::serde_func::serde_to_value(body)?,
        })
    }
}

pub(crate) async fn handle_backend_api_task(
    task: BackendApiTask,
    backend_api: &wallet_transport_backend::api::BackendApi,
    aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
) -> Result<(), crate::ServiceError> {
    match task {
        BackendApiTask::BackendApi(data) => {
            BackendTaskHandle::do_handle(&data.endpoint, data.body, backend_api, aes_cbc_cryptor)
                .await?;
        }
    }
    Ok(())
}
