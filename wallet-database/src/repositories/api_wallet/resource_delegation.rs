use crate::{
    ApiTransactionDbPool,
    dao::api_resource_delegation::ApiResourceDelegationDao,
    entities::api_resource_delegation::{
        ApiResourceDelegationEntity, ApiResourceDelegationResultStatus, NewApiResourceDelegation,
    },
};

pub struct ApiResourceDelegationRepo;

impl ApiResourceDelegationRepo {
    pub async fn upsert(
        pool: &ApiTransactionDbPool,
        input: NewApiResourceDelegation,
    ) -> Result<(), crate::Error> {
        ApiResourceDelegationDao::upsert(pool.write_ref(), input).await
    }

    pub async fn get_by_resource_trade_no(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
    ) -> Result<ApiResourceDelegationEntity, crate::Error> {
        ApiResourceDelegationDao::get_by_resource_trade_no(pool.read_ref(), resource_trade_no).await
    }

    pub async fn list_by_origin_trade_no(
        pool: &ApiTransactionDbPool,
        origin_trade_no: &str,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::list_by_origin_trade_no(pool.read_ref(), origin_trade_no).await
    }

    pub async fn mark_task_ack_sent(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_task_ack_sent(pool.write_ref(), resource_trade_no).await
    }

    pub async fn scan_need_task_ack(
        pool: &ApiTransactionDbPool,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::scan_need_task_ack(pool.read_ref(), limit).await
    }

    pub async fn scan_need_result_ack(
        pool: &ApiTransactionDbPool,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::scan_need_result_ack(pool.read_ref(), limit).await
    }

    pub async fn mark_result_ack_sent(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_result_ack_sent(pool.write_ref(), resource_trade_no).await
    }

    pub async fn mark_result_received(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
        result_status: ApiResourceDelegationResultStatus,
        fail_type: Option<i64>,
        err_code: Option<&str>,
        err_msg: Option<&str>,
        result_payload: Option<&str>,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_result_received(
            pool.write_ref(),
            resource_trade_no,
            result_status,
            fail_type,
            err_code,
            err_msg,
            result_payload,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entities::api_resource_delegation::{
            ApiResourceDelegationOperationType, ApiResourceDelegationSource,
            ApiResourceDelegationStatus,
        },
        repositories::test_helper::setup_api_transaction_pool,
    };

    #[tokio::test]
    async fn resource_delegation_upsert_is_idempotent_and_preserves_source_boundary() {
        let pool = setup_api_transaction_pool("resource_delegation_upsert").await;
        let input = NewApiResourceDelegation::platform_delegate(
            "uid_1",
            "rsc_trade_1",
            "origin_trade_1",
            2,
            "owner",
            "receiver",
            "32000",
        );

        ApiResourceDelegationRepo::upsert(&pool, input.clone()).await.unwrap();
        ApiResourceDelegationRepo::upsert(&pool, input).await.unwrap();

        let got = ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_trade_1")
            .await
            .unwrap();
        assert_eq!(got.source, ApiResourceDelegationSource::Platform);
        assert_eq!(got.operation_type, ApiResourceDelegationOperationType::Delegate);
        assert_eq!(got.origin_trade_no.as_deref(), Some("origin_trade_1"));
        assert_eq!(got.native_amount, "0");
        assert_eq!(got.amount, "32000");

        let list = ApiResourceDelegationRepo::list_by_origin_trade_no(&pool, "origin_trade_1")
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn resource_delegation_task_ack_is_idempotent() {
        let pool = setup_api_transaction_pool("resource_delegation_ack").await;
        let input = NewApiResourceDelegation::platform_delegate(
            "uid_1",
            "rsc_trade_ack",
            "origin_trade_ack",
            1,
            "owner",
            "receiver",
            "1",
        );
        ApiResourceDelegationRepo::upsert(&pool, input).await.unwrap();

        assert_eq!(
            ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_trade_ack").await.unwrap(),
            1
        );
        assert_eq!(
            ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_trade_ack").await.unwrap(),
            0
        );
        let got = ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_trade_ack")
            .await
            .unwrap();
        assert!(got.task_ack_sent_at.is_some());
    }

    #[tokio::test]
    async fn resource_delegation_task_ack_scan_finds_only_unacked_platform_tasks() {
        let pool = setup_api_transaction_pool("resource_delegation_task_ack_scan").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_needs_ack",
                "origin_1",
                2,
                "owner",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_already_acked",
                "origin_2",
                2,
                "owner",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_already_acked").await.unwrap();

        let rows = ApiResourceDelegationRepo::scan_need_task_ack(&pool, 100).await.unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();

        assert!(trade_nos.contains(&"rsc_needs_ack".to_string()));
        assert!(!trade_nos.contains(&"rsc_already_acked".to_string()));
    }

    #[tokio::test]
    async fn resource_delegation_result_records_success_and_failure_facts() {
        let pool = setup_api_transaction_pool("resource_delegation_result").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_success",
                "origin_success",
                2,
                "",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_fail",
                "origin_fail",
                2,
                "",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();

        ApiResourceDelegationRepo::mark_result_received(
            &pool,
            "rsc_success",
            ApiResourceDelegationResultStatus::Success,
            None,
            None,
            None,
            Some("{\"status\":true}"),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::mark_result_received(
            &pool,
            "rsc_fail",
            ApiResourceDelegationResultStatus::Fail,
            Some(2),
            Some("RESOURCE_FAILED"),
            Some("platform delegate failed"),
            Some("{\"status\":false}"),
        )
        .await
        .unwrap();

        let success = ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_success")
            .await
            .unwrap();
        assert_eq!(success.result_status, Some(ApiResourceDelegationResultStatus::Success));
        assert!(success.result_received_at.is_some());
        assert_eq!(success.status, ApiResourceDelegationStatus::Success);

        let fail =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_fail").await.unwrap();
        assert_eq!(fail.result_status, Some(ApiResourceDelegationResultStatus::Fail));
        assert_eq!(fail.fail_type, Some(2));
        assert_eq!(fail.err_code.as_deref(), Some("RESOURCE_FAILED"));
        assert_eq!(fail.err_msg.as_deref(), Some("platform delegate failed"));
        assert_eq!(fail.status, ApiResourceDelegationStatus::Fail);
    }

    #[tokio::test]
    async fn resource_delegation_result_ack_scan_and_mark_are_idempotent() {
        let pool = setup_api_transaction_pool("resource_delegation_result_ack").await;
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_ack",
                "origin_ack",
                2,
                "",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_no_result",
                "origin_no_result",
                2,
                "",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();

        ApiResourceDelegationRepo::mark_result_received(
            &pool,
            "rsc_ack",
            ApiResourceDelegationResultStatus::Success,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let rows = ApiResourceDelegationRepo::scan_need_result_ack(&pool, 100).await.unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();
        assert!(trade_nos.contains(&"rsc_ack".to_string()));
        assert!(!trade_nos.contains(&"rsc_no_result".to_string()));

        assert_eq!(
            ApiResourceDelegationRepo::mark_result_ack_sent(&pool, "rsc_ack").await.unwrap(),
            1
        );
        assert_eq!(
            ApiResourceDelegationRepo::mark_result_ack_sent(&pool, "rsc_ack").await.unwrap(),
            0
        );

        let rows = ApiResourceDelegationRepo::scan_need_result_ack(&pool, 100).await.unwrap();
        assert!(rows.is_empty());
    }
}
