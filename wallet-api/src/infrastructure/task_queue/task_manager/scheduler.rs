use wallet_database::entities::task_queue::KnownTaskName;
use wallet_transport_backend::consts::endpoint::{multisig::*, *};

use crate::infrastructure::task_queue::task::{task_type::TaskType, TaskTrait};

const HISTORICAL_TASK_OFFSET: u8 = 10;

pub const TASK_CATEGORY_LIMIT: &[(TaskType, usize)] = &[
    (TaskType::Initialization, 3),
    (TaskType::BackendApi, 10),
    (TaskType::Mqtt, 35),
    (TaskType::Common, 2),
];

pub(crate) fn assign_priority(
    task: &dyn TaskTrait,
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
fn get_base_priority(task: &dyn TaskTrait) -> Result<u8, crate::ServiceError> {
    let name = task.get_name();

    let priority = match name {
        wallet_database::entities::task_queue::TaskName::Known(known_task_name) => {
            match known_task_name {
                KnownTaskName::PullAnnouncement => 3,
                KnownTaskName::PullHotCoins => 0,
                KnownTaskName::SetBlockBrowserUrl => 0,
                KnownTaskName::SetFiat => 0,
                KnownTaskName::RecoverQueueData => 1,
                KnownTaskName::InitMqtt => 0,

                KnownTaskName::BackendApi => {
                    return extract_backend_priority(task);
                }

                KnownTaskName::OrderMultiSignAccept => 0,
                KnownTaskName::OrderMultiSignAcceptCompleteMsg => 1,
                KnownTaskName::OrderMultiSignServiceComplete => 1,
                KnownTaskName::OrderMultiSignCancel => 0,
                KnownTaskName::MultiSignTransAccept => 0,
                KnownTaskName::MultiSignTransCancel => 0,
                KnownTaskName::MultiSignTransAcceptCompleteMsg => 1,
                KnownTaskName::MultiSignTransExecute => 0,
                KnownTaskName::AcctChange => 3,
                KnownTaskName::OrderMultiSignCreated => 2,
                KnownTaskName::BulletinMsg => 4,
                KnownTaskName::PermissionAccept => 2,
                KnownTaskName::CleanPermission => 2,
                KnownTaskName::QueryCoinPrice => 2, // 中等优先级，通常用于用户操作但不阻塞主流程
                KnownTaskName::QueryQueueResult => 3, // 查询结果，偏后台逻辑，较低优先级
                KnownTaskName::RecoverMultisigAccountData => 1, // 多签账户恢复，重要流程，高优先级
                KnownTaskName::SyncNodesAndLinkToChains => 4, // 链接节点的同步任务，后台操作，较低优先级
                KnownTaskName::OrderAllConfirmed => 1,
                KnownTaskName::UnbindUid => 2,
                KnownTaskName::AddressUse => 2,
                KnownTaskName::ApiWithdraw => 2,
            }
        }
        wallet_database::entities::task_queue::TaskName::Unknown(_) => 0,
    };

    Ok(priority)
}

fn extract_backend_priority(task: &dyn TaskTrait) -> Result<u8, crate::ServiceError> {
    // 使用 downcast_ref 获取 BackendApiTask 类型
    let backend_task = task
        .as_any()
        .downcast_ref::<crate::infrastructure::task_queue::BackendApiTask>()
        .ok_or_else(|| crate::SystemError::Service("BackendApi 类型错误".to_string()))?;

    let priority = match backend_task {
        crate::infrastructure::task_queue::BackendApiTask::BackendApi(backend_api_task_data) => {
            match backend_api_task_data.endpoint.as_str() {
                    DEVICE_INIT
                    | MQTT_INIT
                    | KEYS_RESET
                    | APP_INSTALL_SAVE => 0,
                    // 确认消息，高优先级
                    SEND_MSG_CONFIRM  => 1,
                    // 关键初始化流程，高优先级
                    KEYS_V2_INIT
                    | DEVICE_UPDATE_APP_ID
                    | KEYS_UPDATE_WALLET_NAME
                    // | ADDRESS_INIT
                    | ADDRESS_UPDATE_ACCOUNT_NAME
                    | ADDRESS_BATCH_INIT
                    | DEVICE_EDIT_DEVICE_INVITEE_STATUS
                    | LANGUAGE_INIT
                    | APP_INSTALL_DOWNLOAD
                    | TOKEN_BALANCE_REFRESH
                    | CHAIN_LIST
                    | CHAIN_RPC_LIST => 2,
                    // 重要功能任务，中优先级

                    // DEVICE_BIND_ADDRESS
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
    };

    Ok(priority)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::types::chrono::Utc;
    use wallet_database::entities::task_queue::{KnownTaskName, TaskName, TaskQueueEntity};

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
            TaskName::Known(KnownTaskName::BackendApi),
            r#"{
                "body": null,
                "endpoint": "token/queryRates"
            }"#,
            0,
            0,
        );
        let task2 = make_task_entity(
            "263099222805581824",
            TaskName::Known(KnownTaskName::InitMqtt),
            "",
            1,
            1,
        );

        let task1: Box<dyn TaskTrait> = (&task1).try_into().unwrap();
        let task2: Box<dyn TaskTrait> = (&task2).try_into().unwrap();
        assert_eq!(assign_priority(&*task1, false).unwrap(), 1);
        assert_eq!(assign_priority(&*task2, false).unwrap(), 0);
    }
}
