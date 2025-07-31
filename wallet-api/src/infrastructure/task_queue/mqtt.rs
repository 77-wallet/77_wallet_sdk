use wallet_database::entities::task_queue::{KnownTaskName, TaskName};

use crate::messaging::mqtt::topics;

pub(crate) enum MqttTask {
    OrderMultiSignAccept(topics::OrderMultiSignAccept),
    OrderMultiSignAcceptCompleteMsg(topics::OrderMultiSignAcceptCompleteMsg),
    OrderMultiSignServiceComplete(topics::OrderMultiSignServiceComplete),
    OrderMultiSignCreated(topics::OrderMultiSignCreated),
    OrderAllConfirmed(topics::OrderAllConfirmed),
    OrderMultiSignCancel(topics::OrderMultiSignCancel),
    MultiSignTransAccept(topics::MultiSignTransAccept),
    MultiSignTransCancel(topics::MultiSignTransCancel),
    MultiSignTransAcceptCompleteMsg(topics::MultiSignTransAcceptCompleteMsg),
    MultiSignTransExecute(topics::MultiSignTransExecute),
    AcctChange(topics::AcctChange),
    BulletinMsg(topics::BulletinMsg),
    PermissionAccept(topics::PermissionAccept),
    CleanPermission(topics::CleanPermission),
}

impl MqttTask {
    pub(crate) fn get_name(&self) -> TaskName {
        match self {
            MqttTask::OrderMultiSignAccept(_) => {
                TaskName::Known(KnownTaskName::OrderMultiSignAccept)
            }
            MqttTask::OrderMultiSignAcceptCompleteMsg(_) => {
                TaskName::Known(KnownTaskName::OrderMultiSignAcceptCompleteMsg)
            }
            MqttTask::OrderMultiSignServiceComplete(_) => {
                TaskName::Known(KnownTaskName::OrderMultiSignServiceComplete)
            }
            MqttTask::OrderMultiSignCreated(_) => {
                TaskName::Known(KnownTaskName::OrderMultiSignCreated)
            }
            MqttTask::OrderMultiSignCancel(_) => {
                TaskName::Known(KnownTaskName::OrderMultiSignCancel)
            }
            MqttTask::MultiSignTransAccept(_) => {
                TaskName::Known(KnownTaskName::MultiSignTransAccept)
            }
            MqttTask::MultiSignTransCancel(_) => {
                TaskName::Known(KnownTaskName::MultiSignTransCancel)
            }
            MqttTask::MultiSignTransAcceptCompleteMsg(_) => {
                TaskName::Known(KnownTaskName::MultiSignTransAcceptCompleteMsg)
            }
            MqttTask::AcctChange(_) => TaskName::Known(KnownTaskName::AcctChange),
            MqttTask::BulletinMsg(_) => TaskName::Known(KnownTaskName::BulletinMsg),

            MqttTask::PermissionAccept(_) => TaskName::Known(KnownTaskName::PermissionAccept),
            MqttTask::MultiSignTransExecute(_) => {
                TaskName::Known(KnownTaskName::MultiSignTransExecute)
            }
            MqttTask::CleanPermission(_) => TaskName::Known(KnownTaskName::CleanPermission),
            MqttTask::OrderAllConfirmed(_) => TaskName::Known(KnownTaskName::OrderAllConfirmed),
        }
    }

    pub(crate) fn get_body(&self) -> Result<Option<String>, crate::ServiceError> {
        let res = match self {
            MqttTask::OrderMultiSignAccept(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::OrderMultiSignAcceptCompleteMsg(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::OrderMultiSignServiceComplete(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::OrderMultiSignCancel(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::MultiSignTransAccept(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::MultiSignTransCancel(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::MultiSignTransAcceptCompleteMsg(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::OrderMultiSignCreated(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::AcctChange(req) => Some(wallet_utils::serde_func::serde_to_string(req)?),
            MqttTask::BulletinMsg(req) => Some(wallet_utils::serde_func::serde_to_string(req)?),
            MqttTask::PermissionAccept(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::MultiSignTransExecute(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::OrderAllConfirmed(req) => {
                Some(wallet_utils::serde_func::serde_to_string(req)?)
            }
            MqttTask::CleanPermission(req) => Some(wallet_utils::serde_func::serde_to_string(req)?),
        };
        Ok(res)
    }
}

pub(crate) async fn handle_mqtt_task(
    task: Box<MqttTask>,
    id: &str,
) -> Result<(), crate::ServiceError> {
    match *task {
        MqttTask::OrderMultiSignAccept(data) => data.exec(id).await?,
        MqttTask::OrderMultiSignAcceptCompleteMsg(data) => data.exec(id).await?,
        MqttTask::OrderMultiSignServiceComplete(data) => data.exec(id).await?,
        MqttTask::OrderMultiSignCreated(data) => data.exec(id).await?,
        MqttTask::OrderMultiSignCancel(data) => data.exec(id).await?,
        MqttTask::MultiSignTransAccept(data) => data.exec(id).await?,
        MqttTask::MultiSignTransCancel(data) => data.exec(id).await?,
        MqttTask::MultiSignTransAcceptCompleteMsg(data) => data.exec(id).await?,
        MqttTask::AcctChange(data) => data.exec(id).await?,
        MqttTask::BulletinMsg(data) => data.exec(id).await?,
        MqttTask::PermissionAccept(data) => data.exec(id).await?,
        MqttTask::MultiSignTransExecute(data) => data.exec(id).await?,
        MqttTask::CleanPermission(data) => data.exec(id).await?,
        MqttTask::OrderAllConfirmed(data) => data.exec(id).await?,
    }
    Ok(())
}
