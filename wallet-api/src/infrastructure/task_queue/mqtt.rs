use wallet_database::entities::task_queue::TaskName;

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
    Init(topics::Init),
    BulletinMsg(topics::BulletinMsg),
    PermissionAccept(topics::PermissionAccept),
    CleanPermission(topics::CleanPermission),
}

impl MqttTask {
    pub(crate) fn get_name(&self) -> TaskName {
        match self {
            MqttTask::OrderMultiSignAccept(_) => TaskName::OrderMultiSignAccept,
            MqttTask::OrderMultiSignAcceptCompleteMsg(_) => {
                TaskName::OrderMultiSignAcceptCompleteMsg
            }
            MqttTask::OrderMultiSignServiceComplete(_) => TaskName::OrderMultiSignServiceComplete,
            MqttTask::OrderMultiSignCreated(_) => TaskName::OrderMultiSignCreated,
            MqttTask::OrderMultiSignCancel(_) => TaskName::OrderMultiSignCancel,
            MqttTask::MultiSignTransAccept(_) => TaskName::MultiSignTransAccept,
            MqttTask::MultiSignTransCancel(_) => TaskName::MultiSignTransCancel,
            MqttTask::MultiSignTransAcceptCompleteMsg(_) => {
                TaskName::MultiSignTransAcceptCompleteMsg
            }
            MqttTask::AcctChange(_) => TaskName::AcctChange,
            MqttTask::Init(_) => TaskName::Init,
            MqttTask::BulletinMsg(_) => TaskName::BulletinMsg,

            MqttTask::PermissionAccept(_) => TaskName::PermissionAccept,
            MqttTask::MultiSignTransExecute(_) => TaskName::MultiSignTransExecute,
            MqttTask::CleanPermission(_) => TaskName::CleanPermission,
            MqttTask::OrderAllConfirmed(_) => TaskName::OrderAllConfirmed,
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
            MqttTask::Init(init) => Some(wallet_utils::serde_func::serde_to_string(init)?),
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
        MqttTask::Init(data) => data.exec(id).await?,
        MqttTask::BulletinMsg(data) => data.exec(id).await?,
        MqttTask::PermissionAccept(data) => data.exec(id).await?,
        MqttTask::MultiSignTransExecute(data) => data.exec(id).await?,
        MqttTask::CleanPermission(data) => data.exec(id).await?,
        MqttTask::OrderAllConfirmed(data) => data.exec(id).await?,
    }
    Ok(())
}
