pub(crate) mod task_type;
use std::collections::{BTreeMap, HashMap};

use super::task_manager::dispatcher::PriorityTask;
use crate::{
    infrastructure::task_queue::{task::task_type::TaskType, *},
    messaging::mqtt::topics,
};
use wallet_database::entities::{
    multisig_queue::QueueTaskEntity,
    node::NodeEntity,
    task_queue::{CreateTaskQueueEntity, KnownTaskName, TaskName, TaskQueueEntity},
};
use wallet_database::repositories::task_queue::TaskQueueRepoTrait as _;
use wallet_transport_backend::request::TokenQueryPriceReq;

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

    async fn create_task_entities(
        &self,
    ) -> Result<Vec<CreateTaskQueueEntity>, crate::ServiceError> {
        let mut create_entities = Vec::new();
        for task in self.0.iter() {
            let request_body = task.task.get_request_body()?;
            let create_req = if let Some(id) = &task.id {
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

        Ok(create_entities)
    }

    async fn dispatch_tasks(entities: Vec<TaskQueueEntity>) -> Result<(), crate::ServiceError> {
        let task_sender = crate::manager::Context::get_global_task_manager()?;
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        let mut grouped_tasks: BTreeMap<u8, Vec<TaskQueueEntity>> = BTreeMap::new();

        for task_entity in entities.into_iter() {
            match (&task_entity).try_into() {
                Ok(task) => {
                    let priority = super::task_manager::scheduler::assign_priority(&task, false)?;
                    grouped_tasks.entry(priority).or_default().push(task_entity);
                }
                Err(e) => {
                    tracing::error!("task_entity.try_into() error: {}", e);
                    repo.delete_task(&task_entity.id).await?;
                }
            };
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

    pub(crate) async fn send(self) -> Result<(), crate::ServiceError> {
        if self.0.is_empty() {
            return Ok(());
        }
        let create_entities = self.create_task_entities().await?;
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        let entities = repo.create_multi_task(&create_entities).await?;
        Self::dispatch_tasks(entities).await?;
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

type TaskBuilderFn = fn(&str) -> Result<Task, crate::ServiceError>;

#[macro_export]
macro_rules! register_tasks {
    ($map:ident, $($name:expr => $ty:ty => $constructor:expr),* $(,)?) => {
        $(
            $map.insert($name, |body: &str| {
                let parsed: $ty = wallet_utils::serde_func::serde_from_str(body)?;
                Ok($constructor(parsed))
            });
        )*
    };
}

#[macro_export]
macro_rules! register_tasks_no_parse {
    ($map:ident, $($name:expr => $constructor:expr),* $(,)?) => {
        $(
            $map.insert($name, |_| Ok($constructor));
        )*
    };
}

static TASK_BUILDERS: once_cell::sync::Lazy<
    std::collections::HashMap<KnownTaskName, TaskBuilderFn>,
> = once_cell::sync::Lazy::new(|| {
    let mut map: HashMap<KnownTaskName, TaskBuilderFn> = HashMap::new();

    // Backend + Mqtt + Common：需要解析 request_body 的任务
    register_tasks!(map,
        KnownTaskName::BackendApi => BackendApiTaskData => |parsed| Task::BackendApi(BackendApiTask::BackendApi(parsed)),

        KnownTaskName::OrderMultiSignAccept => topics::OrderMultiSignAccept => |parsed| Task::Mqtt(Box::new(MqttTask::OrderMultiSignAccept(parsed))),
        KnownTaskName::MultiSignTransCancel => topics::MultiSignTransCancel => |parsed| Task::Mqtt(Box::new(MqttTask::MultiSignTransCancel(parsed))),
        KnownTaskName::OrderMultiSignAcceptCompleteMsg => topics::OrderMultiSignAcceptCompleteMsg =>|parsed| Task::Mqtt(Box::new(MqttTask::OrderMultiSignAcceptCompleteMsg(parsed))),
        KnownTaskName::OrderMultiSignServiceComplete => topics::OrderMultiSignServiceComplete => |parsed| Task::Mqtt(Box::new(MqttTask::OrderMultiSignServiceComplete(parsed))),
        KnownTaskName::OrderMultiSignCreated => topics::OrderMultiSignCreated => |parsed| Task::Mqtt(Box::new(MqttTask::OrderMultiSignCreated(parsed))),
        KnownTaskName::OrderMultiSignCancel => topics::OrderMultiSignCancel =>|parsed| Task::Mqtt(Box::new(MqttTask::OrderMultiSignCancel(parsed))),
        KnownTaskName::MultiSignTransAccept => topics::MultiSignTransAccept => |parsed| Task::Mqtt(Box::new(MqttTask::MultiSignTransAccept(parsed))),
        KnownTaskName::MultiSignTransAcceptCompleteMsg => topics::MultiSignTransAcceptCompleteMsg =>|parsed| Task::Mqtt(Box::new(MqttTask::MultiSignTransAcceptCompleteMsg(parsed))),
        KnownTaskName::AcctChange => topics::AcctChange => |parsed| Task::Mqtt(Box::new(MqttTask::AcctChange(parsed))),
        KnownTaskName::BulletinMsg => topics::BulletinMsg => |parsed| Task::Mqtt(Box::new(MqttTask::BulletinMsg(parsed))),
        KnownTaskName::PermissionAccept => topics::PermissionAccept => |parsed| Task::Mqtt(Box::new(MqttTask::PermissionAccept(parsed))),
        KnownTaskName::MultiSignTransExecute => topics::MultiSignTransExecute =>|parsed| Task::Mqtt(Box::new(MqttTask::MultiSignTransExecute(parsed))),
        KnownTaskName::CleanPermission => topics::CleanPermission => |parsed| Task::Mqtt(Box::new(MqttTask::CleanPermission(parsed))),
        KnownTaskName::OrderAllConfirmed => topics::OrderAllConfirmed => |parsed| Task::Mqtt(Box::new(MqttTask::OrderAllConfirmed(parsed))),

        KnownTaskName::QueryCoinPrice => TokenQueryPriceReq => |parsed| Task::Common(CommonTask::QueryCoinPrice(parsed)),
        KnownTaskName::QueryQueueResult => QueueTaskEntity =>|parsed| Task::Common(CommonTask::QueryQueueResult(parsed)),
        KnownTaskName::RecoverMultisigAccountData => RecoverDataBody =>|parsed| Task::Common(CommonTask::RecoverMultisigAccountData(parsed)),
        KnownTaskName::SyncNodesAndLinkToChains => Vec<NodeEntity> =>|parsed| Task::Common(CommonTask::SyncNodesAndLinkToChains(parsed))
    );

    // Initialization：不需要解析 request_body 的任务
    register_tasks_no_parse!(map,
        KnownTaskName::PullAnnouncement => Task::Initialization(InitializationTask::PullAnnouncement),
        KnownTaskName::PullHotCoins => Task::Initialization(InitializationTask::PullHotCoins),
        KnownTaskName::InitTokenPrice => Task::Initialization(InitializationTask::InitTokenPrice),
        KnownTaskName::SetBlockBrowserUrl => Task::Initialization(InitializationTask::SetBlockBrowserUrl),
        KnownTaskName::SetFiat => Task::Initialization(InitializationTask::SetFiat),
        KnownTaskName::RecoverQueueData => Task::Initialization(InitializationTask::RecoverQueueData),
        KnownTaskName::InitMqtt => Task::Initialization(InitializationTask::InitMqtt)
    );

    map
});

impl TryFrom<&TaskQueueEntity> for Task {
    type Error = crate::ServiceError;

    fn try_from(value: &TaskQueueEntity) -> Result<Self, Self::Error> {
        match &value.task_name {
            TaskName::Known(name) => {
                if let Some(builder) = TASK_BUILDERS.get(name) {
                    builder(&value.request_body)
                } else {
                    Err(crate::SystemError::Service(format!("Unknown task: {:?}", name)).into())
                }
            }
            _ => Err(crate::SystemError::Service("Unsupported TaskName type".to_string()).into()),
        }
    }
}

// 将 数据库实体 转换为 Task(根据name匹配对应的类型)
// impl TryFrom<&TaskQueueEntity> for Task {
//     type Error = crate::ServiceError;

//     fn try_from(value: &TaskQueueEntity) -> Result<Self, Self::Error> {
//         match value.task_name {
//             TaskName::Known(KnownTaskName::BackendApi) => {
//                 let api_data =
//                     serde_func::serde_from_str::<BackendApiTaskData>(&value.request_body)?;
//                 Ok(Task::BackendApi(BackendApiTask::BackendApi(api_data)))
//             }
//             TaskName::Known(KnownTaskName::PullAnnouncement) => {
//                 Ok(Task::Initialization(InitializationTask::PullAnnouncement))
//             }
//             TaskName::Known(KnownTaskName::PullHotCoins) => {
//                 Ok(Task::Initialization(InitializationTask::PullHotCoins))
//             }
//             TaskName::Known(KnownTaskName::InitTokenPrice) => {
//                 Ok(Task::Initialization(InitializationTask::InitTokenPrice))
//             }
//             TaskName::Known(KnownTaskName::SetBlockBrowserUrl) => {
//                 Ok(Task::Initialization(InitializationTask::SetBlockBrowserUrl))
//             }
//             TaskName::Known(KnownTaskName::SetFiat) => {
//                 Ok(Task::Initialization(InitializationTask::SetFiat))
//             }
//             // TaskName::ProcessUnconfirmMsg => Ok(Task::Initialization(
//             //     InitializationTask::ProcessUnconfirmMsg,
//             // )),
//             TaskName::Known(KnownTaskName::RecoverQueueData) => {
//                 Ok(Task::Initialization(InitializationTask::RecoverQueueData))
//             }
//             TaskName::Known(KnownTaskName::InitMqtt) => {
//                 Ok(Task::Initialization(InitializationTask::InitMqtt))
//             }
//             TaskName::Known(KnownTaskName::OrderMultiSignAccept) => {
//                 let req = serde_func::serde_from_str::<topics::OrderMultiSignAccept>(
//                     &value.request_body,
//                 )?;
//                 Ok(Task::Mqtt(Box::new(MqttTask::OrderMultiSignAccept(req))))
//             }
//             TaskName::Known(KnownTaskName::MultiSignTransCancel) => {
//                 let req = serde_func::serde_from_str::<topics::MultiSignTransCancel>(
//                     &value.request_body,
//                 )?;
//                 Ok(Task::Mqtt(Box::new(MqttTask::MultiSignTransCancel(req))))
//             }
//             TaskName::Known(KnownTaskName::OrderMultiSignAcceptCompleteMsg) => {
//                 let req = serde_func::serde_from_str::<topics::OrderMultiSignAcceptCompleteMsg>(
//                     &value.request_body,
//                 )?;
//                 Ok(Task::Mqtt(Box::new(
//                     MqttTask::OrderMultiSignAcceptCompleteMsg(req),
//                 )))
//             }
//             TaskName::Known(KnownTaskName::OrderMultiSignServiceComplete) => {
//                 let req = serde_func::serde_from_str::<topics::OrderMultiSignServiceComplete>(
//                     &value.request_body,
//                 )?;
//                 Ok(Task::Mqtt(Box::new(
//                     MqttTask::OrderMultiSignServiceComplete(req),
//                 )))
//             }
//             TaskName::Known(KnownTaskName::OrderMultiSignCreated) => {
//                 let req = serde_func::serde_from_str::<topics::OrderMultiSignCreated>(
//                     &value.request_body,
//                 )?;
//                 Ok(Task::Mqtt(Box::new(MqttTask::OrderMultiSignCreated(req))))
//             }
//             TaskName::Known(KnownTaskName::OrderMultiSignCancel) => {
//                 let req = serde_func::serde_from_str::<topics::OrderMultiSignCancel>(
//                     &value.request_body,
//                 )?;
//                 Ok(Task::Mqtt(Box::new(MqttTask::OrderMultiSignCancel(req))))
//             }
//             TaskName::Known(KnownTaskName::MultiSignTransAccept) => {
//                 let req = serde_func::serde_from_str::<topics::MultiSignTransAccept>(
//                     &value.request_body,
//                 )?;
//                 Ok(Task::Mqtt(Box::new(MqttTask::MultiSignTransAccept(req))))
//             }
//             TaskName::Known(KnownTaskName::MultiSignTransAcceptCompleteMsg) => {
//                 let req = serde_func::serde_from_str::<topics::MultiSignTransAcceptCompleteMsg>(
//                     &value.request_body,
//                 )?;
//                 Ok(Task::Mqtt(Box::new(
//                     MqttTask::MultiSignTransAcceptCompleteMsg(req),
//                 )))
//             }
//             TaskName::Known(KnownTaskName::AcctChange) => {
//                 let req = serde_func::serde_from_str::<topics::AcctChange>(&value.request_body)?;
//                 Ok(Task::Mqtt(Box::new(MqttTask::AcctChange(req))))
//             }
//             TaskName::Known(KnownTaskName::BulletinMsg) => {
//                 let req = serde_func::serde_from_str::<topics::BulletinMsg>(&value.request_body)?;
//                 Ok(Task::Mqtt(Box::new(MqttTask::BulletinMsg(req))))
//             }
//             TaskName::Known(KnownTaskName::QueryCoinPrice) => {
//                 let req = serde_func::serde_from_str::<TokenQueryPriceReq>(&value.request_body)?;
//                 Ok(Task::Common(CommonTask::QueryCoinPrice(req)))
//             }
//             TaskName::Known(KnownTaskName::QueryQueueResult) => {
//                 let req = serde_func::serde_from_str::<QueueTaskEntity>(&value.request_body)?;
//                 Ok(Task::Common(CommonTask::QueryQueueResult(req)))
//             }
//             TaskName::Known(KnownTaskName::RecoverMultisigAccountData) => {
//                 let req = serde_func::serde_from_str::<RecoverDataBody>(&value.request_body)?;
//                 Ok(Task::Common(CommonTask::RecoverMultisigAccountData(req)))
//             }
//             TaskName::Known(KnownTaskName::SyncNodesAndLinkToChains) => {
//                 let req = serde_func::serde_from_str::<Vec<NodeEntity>>(&value.request_body)?;
//                 Ok(Task::Common(CommonTask::SyncNodesAndLinkToChains(req)))
//             }
//             TaskName::Known(KnownTaskName::PermissionAccept) => {
//                 let req =
//                     serde_func::serde_from_str::<topics::PermissionAccept>(&value.request_body)?;
//                 Ok(Task::Mqtt(Box::new(MqttTask::PermissionAccept(req))))
//             }
//             TaskName::Known(KnownTaskName::MultiSignTransExecute) => {
//                 let req = serde_func::serde_from_str::<topics::MultiSignTransExecute>(
//                     &value.request_body,
//                 )?;
//                 Ok(Task::Mqtt(Box::new(MqttTask::MultiSignTransExecute(req))))
//             }
//             TaskName::Known(KnownTaskName::CleanPermission) => {
//                 let req =
//                     serde_func::serde_from_str::<topics::CleanPermission>(&value.request_body)?;
//                 Ok(Task::Mqtt(Box::new(MqttTask::CleanPermission(req))))
//             }
//             TaskName::Known(KnownTaskName::OrderAllConfirmed) => {
//                 let req =
//                     serde_func::serde_from_str::<topics::OrderAllConfirmed>(&value.request_body)?;
//                 Ok(Task::Mqtt(Box::new(MqttTask::OrderAllConfirmed(req))))
//             }
//             _ => Err(crate::SystemError::Service("Unknown task name".to_string()).into()),
//         }
//     }
// }
