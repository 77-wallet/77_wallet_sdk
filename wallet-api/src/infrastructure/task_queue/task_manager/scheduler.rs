use wallet_database::entities::task_queue::TaskQueueEntity;

use crate::infrastructure::task_queue::Task;

const HISTORICAL_TASK_OFFSET: u8 = 10;

pub(crate) fn assign_priority(task: &TaskQueueEntity) -> Result<u8, crate::ServiceError> {
    let base = get_base_priority(task)?; // 任务类型对应基础优先级，比如 sync=1，其他=3
    Ok(base.saturating_add(HISTORICAL_TASK_OFFSET)) // 历史任务统一加偏移，确保比新任务优先级低
}

/// 动态确定任务优先级（0 = 高，1 = 中，2 = 低）
fn get_base_priority(task: &TaskQueueEntity) -> Result<u8, crate::ServiceError> {
    let task: Task = task.try_into()?;
    Ok(match task {
        Task::Initialization(initialization_task) => 0,
        Task::BackendApi(backend_api_task) => 1,
        Task::Mqtt(mqtt_task) => 1,
        Task::Common(common_task) => 2,
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

        assert_eq!(assign_priority(&task1).unwrap(), 1);
        assert_eq!(assign_priority(&task2).unwrap(), 0);
    }
}
