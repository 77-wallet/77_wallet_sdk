use wallet_database::entities::task_queue::TaskQueueEntity;
use wallet_transport_backend::consts::endpoint::{multisig::*, *};

use crate::infrastructure::task_queue::{CommonTask, InitializationTask, MqttTask, Task, TaskType};

const HISTORICAL_TASK_OFFSET: u8 = 10;

pub const TASK_CATEGORY_LIMIT: &[(TaskType, usize)] = &[
    (TaskType::Initialization, 1),
    (TaskType::BackendApi, 4),
    (TaskType::Mqtt, 2),
    (TaskType::Common, 1),
];

pub(crate) fn assign_priority(
    task: &TaskQueueEntity,
    is_history: bool,
) -> Result<u8, crate::ServiceError> {
    let base = get_base_priority(task)?; // 任务类型对应基础优先级，比如 sync=1，其他=3
    Ok(if is_history {
        // 历史任务统一加偏移，确保比新任务优先级低
        base + HISTORICAL_TASK_OFFSET
    } else {
        base
    })
}

/// 动态确定任务优先级（0 = 高，1 = 中，2 = 低）
fn get_base_priority(task: &TaskQueueEntity) -> Result<u8, crate::ServiceError> {
    let task: Task = task.try_into()?;
    Ok(match task {
        Task::Initialization(initialization_task) => match initialization_task {
            InitializationTask::PullAnnouncement => 3,
            InitializationTask::PullHotCoins => 0,
            InitializationTask::InitTokenPrice => 1,
            InitializationTask::ProcessUnconfirmMsg => 2,
            InitializationTask::SetBlockBrowserUrl => 0,
            InitializationTask::SetFiat => 0,
            InitializationTask::RecoverQueueData => 1,
            InitializationTask::InitMqtt => 0,
        },
        Task::BackendApi(backend_api_task) => match backend_api_task {
            crate::infrastructure::task_queue::BackendApiTask::BackendApi(
                backend_api_task_data,
            ) => {
                match backend_api_task_data.endpoint.as_str() {
                    DEVICE_INIT => 0,
                    // 确认消息，高优先级
                    SEND_MSG_CONFIRM  => 1,
                    // 关键初始化流程，高优先级
                    KEYS_INIT | ADDRESS_INIT | LANGUAGE_INIT | MQTT_INIT|
                    DEVICE_EDIT_DEVICE_INVITEE_STATUS| APP_INSTALL_DOWNLOAD | CHAIN_LIST
                    | CHAIN_RPC_LIST => 2,
                    // 重要功能任务，中优先级
                    DEVICE_BIND_ADDRESS
                    | DEVICE_UNBIND_ADDRESS
                    | DEVICE_DELETE
                    | SYS_CONFIG_FIND_CONFIG_BY_KEY
                    | TOKEN_QUERY_RATES
                    | TOKEN_CUSTOM_TOKEN_INIT
                    => 3,
                    SIGNED_ORDER_ACCEPT
                    | SIGNED_ORDER_CANCEL
                    | SIGNED_TRAN_CREATE
                    | SIGNED_TRAN_ACCEPT
                    | SIGNED_ORDER_UPDATE_RECHARGE_HASH
                    | SIGNED_ORDER_UPDATE_SIGNED_HASH
                    | SIGNED_TRAN_UPDATE_TRANS_HASH
                    | SIGNED_ORDER_SAVE_RAW_DATA => 4,
                    PERMISSION_ACCEPT
                    | UPLOAD_PERMISSION_TRANS => 5,


                    // 默认 endpoint，次要任务或后台同步任务
                    _ if crate::infrastructure::task_queue::task_handle::backend_handle::BackendTaskHandle::is_default_endpoint(&backend_api_task_data.endpoint) => 6,

                    // 其它未知/扩展任务，最低优先级
                    _ => 7,
                }
            }
        },
        Task::Mqtt(mqtt_task) => match *mqtt_task {
            MqttTask::OrderMultiSignAccept(_) => 0,
            MqttTask::OrderMultiSignAcceptCompleteMsg(_) => 1,
            MqttTask::OrderMultiSignServiceComplete(_) => 1,
            MqttTask::OrderMultiSignCreated(_) => 2,
            MqttTask::OrderAllConfirmed(_) => 1,
            MqttTask::OrderMultiSignCancel(_) => 0,
            MqttTask::MultiSignTransAccept(_) => 0,
            MqttTask::MultiSignTransCancel(_) => 0,
            MqttTask::MultiSignTransAcceptCompleteMsg(_) => 1,
            MqttTask::MultiSignTransExecute(_) => 0,
            MqttTask::AcctChange(_) => 3,
            MqttTask::Init(_) => 0,
            MqttTask::BulletinMsg(_) => 4,
            MqttTask::PermissionAccept(_) => 2,
            MqttTask::CleanPermission(_) => 2,
        },
        Task::Common(common_task) => match common_task {
            CommonTask::QueryCoinPrice(_) => 2, // 中等优先级，通常用于用户操作但不阻塞主流程
            CommonTask::QueryQueueResult(_) => 3, // 查询结果，偏后台逻辑，较低优先级
            CommonTask::RecoverMultisigAccountData(_) => 1, // 多签账户恢复，重要流程，高优先级
            CommonTask::SyncNodesAndLinkToChains(_) => 4, // 链接节点的同步任务，后台操作，较低优先级
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::types::chrono::Utc;
    use wallet_database::entities::task_queue::{TaskName, TaskQueueEntity};

    fn make_task_entity(
        id: &str,
        task_name: TaskName,
        request_body: &str,
        task_type: u8,
        status: u8,
    ) -> TaskQueueEntity {
        TaskQueueEntity {
            id: id.to_string(),
            task_name,
            request_body: request_body.to_string(), // 简单示例，可以根据需要填充 JSON 字符串
            r#type: task_type,
            status,
            created_at: Utc::now(),
            updated_at: None,
        }
    }

    #[test]
    fn test_assign_priority() {
        let task1 = make_task_entity(
            "263099225674485766",
            TaskName::BackendApi,
            r#"{
                "body": null,
                "endpoint": "token/queryRates"
            }"#,
            0,
            0,
        );
        let task2 = make_task_entity("263099222805581824", TaskName::InitMqtt, "", 1, 1);

        assert_eq!(assign_priority(&task1, false).unwrap(), 1);
        assert_eq!(assign_priority(&task2, false).unwrap(), 0);
    }
}
