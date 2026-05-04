use crate::{
    ApiTransactionDbPool,
    dao::api_resource_operation::ApiResourceOperationDao,
    entities::api_resource_operation::{ApiResourceOperationEntity, NewApiResourceOperation},
};

pub struct ApiResourceOperationRepo;

impl ApiResourceOperationRepo {
    pub async fn upsert(
        pool: &ApiTransactionDbPool,
        input: NewApiResourceOperation,
    ) -> Result<(), crate::Error> {
        ApiResourceOperationDao::upsert(pool.write_ref(), input).await
    }

    pub async fn get_by_resource_trade_no(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
    ) -> Result<ApiResourceOperationEntity, crate::Error> {
        ApiResourceOperationDao::get_by_resource_trade_no(pool.read_ref(), resource_trade_no).await
    }

    pub async fn mark_task_ack_sent(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceOperationDao::mark_task_ack_sent(pool.write_ref(), resource_trade_no).await
    }

    pub async fn scan_need_task_ack(
        pool: &ApiTransactionDbPool,
        limit: usize,
    ) -> Result<Vec<ApiResourceOperationEntity>, crate::Error> {
        ApiResourceOperationDao::scan_need_task_ack(pool.read_ref(), limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entities::{
            api_resource_operation::{
                ApiResourceOperationStatus, ApiResourceOperationTaskSource,
                ApiResourceOperationType,
            },
            api_resource_type::ApiResourceType,
        },
        repositories::test_helper::setup_api_transaction_pool,
    };

    #[tokio::test]
    async fn resource_operation_upsert_supports_backend_stake_task() {
        let pool = setup_api_transaction_pool("resource_operation_upsert").await;
        let input = NewApiResourceOperation::backend_stake("uid_1", "op_trade_1", "owner", "1000");

        ApiResourceOperationRepo::upsert(&pool, input.clone()).await.unwrap();
        ApiResourceOperationRepo::upsert(&pool, input).await.unwrap();

        let got =
            ApiResourceOperationRepo::get_by_resource_trade_no(&pool, "op_trade_1").await.unwrap();
        assert_eq!(got.task_source, ApiResourceOperationTaskSource::Backend);
        assert_eq!(got.operation_type, ApiResourceOperationType::Stake);
        assert_eq!(got.resource_type, ApiResourceType::Energy);
        assert_eq!(got.status, ApiResourceOperationStatus::Pending);
        assert_eq!(got.amount, "1000");
    }

    #[tokio::test]
    async fn resource_operation_task_ack_is_idempotent() {
        let pool = setup_api_transaction_pool("resource_operation_ack").await;
        let input = NewApiResourceOperation::backend_stake("uid_1", "op_trade_ack", "owner", "1");
        ApiResourceOperationRepo::upsert(&pool, input).await.unwrap();

        assert_eq!(
            ApiResourceOperationRepo::mark_task_ack_sent(&pool, "op_trade_ack").await.unwrap(),
            1
        );
        let got = ApiResourceOperationRepo::get_by_resource_trade_no(&pool, "op_trade_ack")
            .await
            .unwrap();
        assert!(got.task_ack_sent_at.is_some());
    }

    #[tokio::test]
    async fn resource_operation_task_ack_scan_selects_unsent_backend_tasks() {
        let pool = setup_api_transaction_pool("resource_operation_ack_scan").await;
        ApiResourceOperationRepo::upsert(
            &pool,
            NewApiResourceOperation::backend_stake("uid_1", "op_need_ack", "owner", "1"),
        )
        .await
        .unwrap();
        ApiResourceOperationRepo::upsert(
            &pool,
            NewApiResourceOperation::backend_stake("uid_1", "op_sent_ack", "owner", "1"),
        )
        .await
        .unwrap();
        ApiResourceOperationRepo::mark_task_ack_sent(&pool, "op_sent_ack").await.unwrap();

        let rows = ApiResourceOperationRepo::scan_need_task_ack(&pool, 100).await.unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();
        assert!(trade_nos.contains(&"op_need_ack".to_string()));
        assert!(!trade_nos.contains(&"op_sent_ack".to_string()));
    }
}
