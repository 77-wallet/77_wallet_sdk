use wallet_database::entities::task_queue::{KnownTaskName, TaskName};

use crate::infrastructure::task_queue::task::{task_type::TaskType, TaskTrait};

use super::task_handle::backend_handle::BackendTaskHandle;

#[async_trait::async_trait]
impl TaskTrait for BackendApiTask {
    fn get_name(&self) -> TaskName {
        self.get_name()
    }
    fn get_type(&self) -> TaskType {
        TaskType::BackendApi
    }
    fn get_body(&self) -> Result<Option<String>, crate::ServiceError> {
        self.get_body()
    }

    async fn execute(&self, _id: &str) -> Result<(), crate::ServiceError> {
        let backend_api = crate::manager::Context::get_global_backend_api()?;
        match self {
            BackendApiTask::BackendApi(data) => {
                BackendTaskHandle::do_handle(&data.endpoint, data.body.clone(), backend_api)
                    .await?;
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(crate) enum BackendApiTask {
    BackendApi(BackendApiTaskData),
}

impl BackendApiTask {
    pub(crate) fn get_name(&self) -> TaskName {
        TaskName::Known(KnownTaskName::BackendApi)
    }

    pub(crate) fn get_body(&self) -> Result<Option<String>, crate::ServiceError> {
        match self {
            BackendApiTask::BackendApi(api_data) => {
                Ok(Some(wallet_utils::serde_func::serde_to_string(api_data)?))
            }
        }
    }
}

// impl BackendApiTask {
//     pub fn new<T>(endpoint: &str, body: &T) -> Result<Self, crate::ServiceError>
//     where
//         T: serde::Serialize,
//     {
//         Ok(Self::BackendApi(BackendApiTaskData::new(endpoint, body)?))
//     }
// }

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
