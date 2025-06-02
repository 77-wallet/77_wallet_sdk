mod task_handle;
pub(crate) mod task_manager;
use std::collections::BTreeMap;

use crate::messaging::mqtt::topics;
use task_manager::dispatcher::PriorityTask;
use wallet_database::entities::{
    multisig_queue::QueueTaskEntity,
    node::NodeEntity,
    task_queue::{TaskName, TaskQueueEntity},
};
use wallet_transport_backend::request::TokenQueryPriceReq;

pub(crate) mod initialization;
pub(crate) use initialization::*;

pub(crate) mod backend;
pub(crate) use backend::*;

pub(crate) mod mqtt;
pub(crate) use mqtt::*;

pub(crate) mod common;
pub(crate) use common::*;
use wallet_utils::serde_func;

pub(crate) struct TaskItem {
    pub(crate) id: Option<String>,
    pub(crate) task: Task,
}

impl TaskItem {
    pub fn new(task: Task) -> Self {
        Self { id: None, task }
    }

    pub fn new_with_id(id: &str, task: Task) -> Self {
        Self {
            id: Some(id.to_string()),
            task,
        }
    }
}

pub(crate) struct Tasks(Vec<TaskItem>);

impl Tasks {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn push(mut self, task: Task) -> Self {
        self.0.push(TaskItem::new(task));
        self
    }

    pub fn push_with_id(mut self, id: &str, task: Task) -> Self {
        self.0.push(TaskItem::new_with_id(id, task));
        self
    }

    pub(crate) async fn send(self) -> Result<(), crate::ServiceError> {
        use wallet_database::repositories::task_queue::TaskQueueRepoTrait as _;
        let task_sender = crate::manager::Context::get_global_task_manager()?;
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        if self.0.is_empty() {
            return Ok(());
        }

        let mut create_entities = Vec::new();
        for task in self.0 {
            let request_body = task.task.get_request_body()?;
            let create_req = if let Some(id) = task.id {
                wallet_database::entities::task_queue::CreateTaskQueueEntity::with_mqtt_request_string(
                    id,
                    task.task.get_name(),
                    request_body,
                )?
            } else {
                wallet_database::entities::task_queue::CreateTaskQueueEntity::with_backend_request_string(
                    task.task.get_name(),
                    request_body,
                )?
            };
            create_entities.push(create_req);
        }

        let entities = repo.create_multi_task(&create_entities).await?;

        let mut grouped_tasks: BTreeMap<u8, Vec<TaskQueueEntity>> = BTreeMap::new();

        for task in entities.into_iter() {
            let priority = task_manager::scheduler::assign_priority(&task, false)?;
            grouped_tasks.entry(priority).or_default().push(task);
        }

        for (priority, tasks) in grouped_tasks {
            if let Err(e) = task_sender
                .get_task_sender()
                .send(PriorityTask { priority, tasks })
            {
                tracing::error!("send task queue error: {}", e);
            }
        }

        repo.delete_oldest_by_status_when_exceeded(200000, 2)
            .await?;
        Ok(())
    }
}

pub(crate) enum Task {
    Initialization(InitializationTask),
    BackendApi(BackendApiTask),
    Mqtt(Box<MqttTask>),
    Common(CommonTask),
}

impl Task {
    pub fn get_name(&self) -> TaskName {
        match self {
            Task::Initialization(task) => task.get_name(),
            Task::BackendApi(task) => task.get_name(),
            Task::Mqtt(task) => task.get_name(),
            Task::Common(task) => task.get_name(),
        }
    }

    pub fn get_request_body(&self) -> Result<Option<String>, crate::ServiceError> {
        Ok(match self {
            Task::Initialization(task) => task.get_body()?,
            Task::BackendApi(task) => task.get_body()?,
            Task::Mqtt(task) => task.get_body()?,
            Task::Common(task) => task.get_body()?,
        })
    }

    pub fn get_type(&self) -> TaskType {
        match self {
            Task::Initialization(_) => TaskType::Initialization,
            Task::BackendApi(_) => TaskType::BackendApi,
            Task::Mqtt(_) => TaskType::Mqtt,
            Task::Common(_) => TaskType::Common,
        }
    }
}

// 将 数据库实体 转换为 Task(根据name匹配对应的类型)
impl TryFrom<&TaskQueueEntity> for Task {
    type Error = crate::ServiceError;

    fn try_from(value: &TaskQueueEntity) -> Result<Self, Self::Error> {
        match value.task_name {
            TaskName::BackendApi => {
                let api_data =
                    serde_func::serde_from_str::<BackendApiTaskData>(&value.request_body)?;
                Ok(Task::BackendApi(BackendApiTask::BackendApi(api_data)))
            }
            TaskName::PullAnnouncement => {
                Ok(Task::Initialization(InitializationTask::PullAnnouncement))
            }
            TaskName::PullHotCoins => Ok(Task::Initialization(InitializationTask::PullHotCoins)),
            TaskName::InitTokenPrice => {
                Ok(Task::Initialization(InitializationTask::InitTokenPrice))
            }
            TaskName::SetBlockBrowserUrl => {
                Ok(Task::Initialization(InitializationTask::SetBlockBrowserUrl))
            }
            TaskName::SetFiat => Ok(Task::Initialization(InitializationTask::SetFiat)),
            // TaskName::ProcessUnconfirmMsg => Ok(Task::Initialization(
            //     InitializationTask::ProcessUnconfirmMsg,
            // )),
            TaskName::RecoverQueueData => {
                Ok(Task::Initialization(InitializationTask::RecoverQueueData))
            }
            TaskName::InitMqtt => Ok(Task::Initialization(InitializationTask::InitMqtt)),
            TaskName::OrderMultiSignAccept => {
                let req = serde_func::serde_from_str::<topics::OrderMultiSignAccept>(
                    &value.request_body,
                )?;
                Ok(Task::Mqtt(Box::new(MqttTask::OrderMultiSignAccept(req))))
            }
            TaskName::MultiSignTransCancel => {
                let req = serde_func::serde_from_str::<topics::MultiSignTransCancel>(
                    &value.request_body,
                )?;
                Ok(Task::Mqtt(Box::new(MqttTask::MultiSignTransCancel(req))))
            }
            TaskName::OrderMultiSignAcceptCompleteMsg => {
                let req = serde_func::serde_from_str::<topics::OrderMultiSignAcceptCompleteMsg>(
                    &value.request_body,
                )?;
                Ok(Task::Mqtt(Box::new(
                    MqttTask::OrderMultiSignAcceptCompleteMsg(req),
                )))
            }
            TaskName::OrderMultiSignServiceComplete => {
                let req = serde_func::serde_from_str::<topics::OrderMultiSignServiceComplete>(
                    &value.request_body,
                )?;
                Ok(Task::Mqtt(Box::new(
                    MqttTask::OrderMultiSignServiceComplete(req),
                )))
            }
            TaskName::OrderMultiSignCreated => {
                let req = serde_func::serde_from_str::<topics::OrderMultiSignCreated>(
                    &value.request_body,
                )?;
                Ok(Task::Mqtt(Box::new(MqttTask::OrderMultiSignCreated(req))))
            }
            TaskName::OrderMultiSignCancel => {
                let req = serde_func::serde_from_str::<topics::OrderMultiSignCancel>(
                    &value.request_body,
                )?;
                Ok(Task::Mqtt(Box::new(MqttTask::OrderMultiSignCancel(req))))
            }
            TaskName::MultiSignTransAccept => {
                let req = serde_func::serde_from_str::<topics::MultiSignTransAccept>(
                    &value.request_body,
                )?;
                Ok(Task::Mqtt(Box::new(MqttTask::MultiSignTransAccept(req))))
            }
            TaskName::MultiSignTransAcceptCompleteMsg => {
                let req = serde_func::serde_from_str::<topics::MultiSignTransAcceptCompleteMsg>(
                    &value.request_body,
                )?;
                Ok(Task::Mqtt(Box::new(
                    MqttTask::MultiSignTransAcceptCompleteMsg(req),
                )))
            }
            TaskName::AcctChange => {
                let req = serde_func::serde_from_str::<topics::AcctChange>(&value.request_body)?;
                Ok(Task::Mqtt(Box::new(MqttTask::AcctChange(req))))
            }
            TaskName::Init => {
                let req = serde_func::serde_from_str::<topics::Init>(&value.request_body)?;
                Ok(Task::Mqtt(Box::new(MqttTask::Init(req))))
            }
            TaskName::BulletinMsg => {
                let req = serde_func::serde_from_str::<topics::BulletinMsg>(&value.request_body)?;
                Ok(Task::Mqtt(Box::new(MqttTask::BulletinMsg(req))))
            }
            TaskName::QueryCoinPrice => {
                let req = serde_func::serde_from_str::<TokenQueryPriceReq>(&value.request_body)?;
                Ok(Task::Common(CommonTask::QueryCoinPrice(req)))
            }
            TaskName::QueryQueueResult => {
                let req = serde_func::serde_from_str::<QueueTaskEntity>(&value.request_body)?;
                Ok(Task::Common(CommonTask::QueryQueueResult(req)))
            }
            TaskName::RecoverMultisigAccountData => {
                let req = serde_func::serde_from_str::<RecoverDataBody>(&value.request_body)?;
                Ok(Task::Common(CommonTask::RecoverMultisigAccountData(req)))
            }
            TaskName::SyncNodesAndLinkToChains => {
                let req = serde_func::serde_from_str::<Vec<NodeEntity>>(&value.request_body)?;
                Ok(Task::Common(CommonTask::SyncNodesAndLinkToChains(req)))
            }
            TaskName::PermissionAccept => {
                let req =
                    serde_func::serde_from_str::<topics::PermissionAccept>(&value.request_body)?;
                Ok(Task::Mqtt(Box::new(MqttTask::PermissionAccept(req))))
            }
            TaskName::MultiSignTransExecute => {
                let req = serde_func::serde_from_str::<topics::MultiSignTransExecute>(
                    &value.request_body,
                )?;
                Ok(Task::Mqtt(Box::new(MqttTask::MultiSignTransExecute(req))))
            }
            TaskName::CleanPermission => {
                let req =
                    serde_func::serde_from_str::<topics::CleanPermission>(&value.request_body)?;
                Ok(Task::Mqtt(Box::new(MqttTask::CleanPermission(req))))
            }
            TaskName::OrderAllConfirmed => {
                let req =
                    serde_func::serde_from_str::<topics::OrderAllConfirmed>(&value.request_body)?;
                Ok(Task::Mqtt(Box::new(MqttTask::OrderAllConfirmed(req))))
            }
        }
    }
}

/// 0: initialization, 1: backend_api, 2: mqtt
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) enum TaskType {
    Initialization,
    BackendApi,
    Mqtt,
    Common,
}

impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for TaskType {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> sqlx::Result<Self> {
        use sqlx::Row as _;
        let value = row.try_get::<i64, _>("type")?;
        match value {
            0 => Ok(TaskType::Initialization),
            1 => Ok(TaskType::BackendApi),
            2 => Ok(TaskType::Mqtt),
            3 => Ok(TaskType::Common),
            _ => Err(sqlx::Error::RowNotFound),
        }
    }
}

impl sqlx::Encode<'_, sqlx::sqlite::Sqlite> for TaskType {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::sqlite::Sqlite as sqlx::database::HasArguments<'_>>::ArgumentBuffer,
    ) -> sqlx::encode::IsNull {
        match self {
            TaskType::Initialization => buf.push(sqlx::sqlite::SqliteArgumentValue::Int64(0)),
            TaskType::BackendApi => buf.push(sqlx::sqlite::SqliteArgumentValue::Int64(1)),
            TaskType::Mqtt => buf.push(sqlx::sqlite::SqliteArgumentValue::Int64(2)),
            TaskType::Common => buf.push(sqlx::sqlite::SqliteArgumentValue::Int64(3)),
        }
        sqlx::encode::IsNull::No
    }
}

impl sqlx::Decode<'_, sqlx::sqlite::Sqlite> for TaskType {
    fn decode(
        value: <sqlx::sqlite::Sqlite as sqlx::database::HasValueRef<'_>>::ValueRef,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let value = <i64 as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        match value {
            0 => Ok(TaskType::Initialization),
            1 => Ok(TaskType::BackendApi),
            2 => Ok(TaskType::Mqtt),
            3 => Ok(TaskType::Common),
            _ => Err(Box::new(sqlx::Error::ColumnNotFound(
                "Invalid TaskType value".into(),
            ))),
        }
    }
}
