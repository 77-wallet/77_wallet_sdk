use crate::{
    ApiTransactionDbPool,
    dao::api_resource_delegation::ApiResourceDelegationDao,
    entities::api_resource_delegation::{
        ApiResourceDelegationEntity, ApiResourceDelegationOperationType,
        ApiResourceDelegationRecoverStatus, ApiResourceDelegationResultStatus,
        ApiResourceDelegationSource, NewApiResourceDelegation,
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

    pub async fn find_by_resource_trade_no(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
    ) -> Result<Option<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::find_by_resource_trade_no(pool.read_ref(), resource_trade_no)
            .await
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
        Self::scan_need_task_ack_for_origin_type(
            pool,
            crate::entities::api_trade_type::ApiTradeType::Collect as i64,
            limit,
        )
        .await
    }

    pub async fn scan_need_task_ack_for_origin_type(
        pool: &ApiTransactionDbPool,
        origin_trade_type: i64,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        // 这里只扫描“后端下发的平台代理任务 ACK”。
        // 商户侧资源 gate 只记录原单依赖的 backend dlTradeNo，不再写 platform
        // placeholder；因此扫描边界只看 source/operation/origin_type。
        // 本地 fallback 不走后端任务 ACK，也不会混进这组扫描。
        ApiResourceDelegationDao::scan_need_task_ack_by_origin_type(
            pool.read_ref(),
            origin_trade_type,
            limit,
        )
        .await
    }

    pub async fn scan_need_task_ack_for_origin_type_source_and_operation(
        pool: &ApiTransactionDbPool,
        origin_trade_type: i64,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::scan_need_task_ack_by_origin_type_source_and_operation(
            pool.read_ref(),
            origin_trade_type,
            source,
            operation_type,
            limit,
        )
        .await
    }

    pub async fn scan_need_result_ack_for_origin_type(
        pool: &ApiTransactionDbPool,
        origin_trade_type: i64,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        // 这里只扫描“后端下发的平台代理结果 ACK”。
        // 商户侧不会创建 platform placeholder；Local fallback 没有这条后端结果确认副作用。
        ApiResourceDelegationDao::scan_need_result_ack_by_origin_type(
            pool.read_ref(),
            origin_trade_type,
            limit,
        )
        .await
    }

    pub async fn find_pending_result_ack_by_origin(
        pool: &ApiTransactionDbPool,
        origin_trade_type: i64,
        origin_trade_no: &str,
    ) -> Result<Option<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::find_pending_result_ack_by_origin(
            pool.read_ref(),
            origin_trade_type,
            origin_trade_no,
        )
        .await
    }

    pub async fn scan_need_result_ack_for_source_and_operation(
        pool: &ApiTransactionDbPool,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::scan_need_result_ack_by_source_and_operation(
            pool.read_ref(),
            source,
            operation_type,
            limit,
        )
        .await
    }

    pub async fn mark_result_ack_retry_wait(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
        next_retry_at: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_result_ack_retry_wait(
            pool.write_ref(),
            resource_trade_no,
            next_retry_at,
        )
        .await
    }

    pub async fn scan_need_result_ack(
        pool: &ApiTransactionDbPool,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        Self::scan_need_result_ack_for_origin_type(
            pool,
            crate::entities::api_trade_type::ApiTradeType::Collect as i64,
            limit,
        )
        .await
    }

    pub async fn scan_can_execute_for_origin_type(
        pool: &ApiTransactionDbPool,
        origin_trade_type: i64,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        // 默认执行扫描只面向 platform source，保持历史调用语义不变。
        ApiResourceDelegationDao::scan_can_execute_by_origin_type(
            pool.read_ref(),
            origin_trade_type,
            limit,
        )
        .await
    }

    pub async fn scan_can_execute(
        pool: &ApiTransactionDbPool,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        Self::scan_can_execute_for_origin_type(
            pool,
            crate::entities::api_trade_type::ApiTradeType::Collect as i64,
            limit,
        )
        .await
    }

    pub async fn scan_can_execute_for_origin_type_and_source(
        pool: &ApiTransactionDbPool,
        origin_trade_type: i64,
        source: ApiResourceDelegationSource,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        // 新调用方需要同时声明“主流程归属 + 资源来源”。
        // 这用于 collect local fallback 这类共享副链中的第二层边界。
        ApiResourceDelegationDao::scan_can_execute_by_origin_type_and_source(
            pool.read_ref(),
            origin_trade_type,
            source,
            limit,
        )
        .await
    }

    pub async fn scan_can_execute_for_origin_type_source_and_operation(
        pool: &ApiTransactionDbPool,
        origin_trade_type: i64,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::scan_can_execute_by_origin_type_source_and_operation(
            pool.read_ref(),
            origin_trade_type,
            source,
            operation_type,
            limit,
        )
        .await
    }

    pub async fn scan_can_recover_local_undelegation_for_origin_type(
        pool: &ApiTransactionDbPool,
        origin_trade_type: i64,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::scan_can_recover_local_undelegation_by_origin_type(
            pool.read_ref(),
            origin_trade_type,
            limit,
        )
        .await
    }

    pub async fn scan_can_recover_by_origin_type_source_and_operation(
        pool: &ApiTransactionDbPool,
        origin_trade_type: i64,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::scan_can_recover_by_origin_type_source_and_operation(
            pool.read_ref(),
            origin_trade_type,
            source,
            operation_type,
            limit,
        )
        .await
    }

    pub async fn claim_build_slot(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::claim_build_slot(pool.write_ref(), resource_trade_no).await
    }

    pub async fn mark_broadcast_success(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
        tx_hash: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_broadcast_success(
            pool.write_ref(),
            resource_trade_no,
            tx_hash,
        )
        .await
    }

    pub async fn scan_need_tx_exec_receipt_upload_for_origin_type(
        pool: &ApiTransactionDbPool,
        origin_trade_type: i64,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        // 当前 receipt upload 仍只对应平台代理事实。
        // local fallback 终态由主链本地执行结果直接投影，不走后端资源回执。
        ApiResourceDelegationDao::scan_need_tx_exec_receipt_upload_by_origin_type(
            pool.read_ref(),
            origin_trade_type,
            limit,
        )
        .await
    }

    pub async fn scan_need_tx_exec_receipt_upload_for_origin_type_source_and_operation(
        pool: &ApiTransactionDbPool,
        origin_trade_type: i64,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::scan_need_tx_exec_receipt_upload_by_origin_type_source_and_operation(
            pool.read_ref(),
            origin_trade_type,
            source,
            operation_type,
            limit,
        )
        .await
    }

    pub async fn scan_need_tx_exec_receipt_upload_for_source_and_operation(
        pool: &ApiTransactionDbPool,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::scan_need_tx_exec_receipt_upload_by_source_and_operation(
            pool.read_ref(),
            source,
            operation_type,
            limit,
        )
        .await
    }

    pub async fn scan_need_tx_exec_receipt_upload(
        pool: &ApiTransactionDbPool,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        Self::scan_need_tx_exec_receipt_upload_for_origin_type(
            pool,
            crate::entities::api_trade_type::ApiTradeType::Collect as i64,
            limit,
        )
        .await
    }

    pub async fn mark_tx_exec_receipt_uploaded(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_tx_exec_receipt_uploaded(pool.write_ref(), resource_trade_no)
            .await
    }

    pub async fn mark_tx_exec_receipt_uploaded_for_source_and_operation(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_tx_exec_receipt_uploaded_by_source_and_operation(
            pool.write_ref(),
            resource_trade_no,
            source,
            operation_type,
        )
        .await
    }

    pub async fn mark_failed_if_unfinished(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
        err_code: &str,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_failed_if_unfinished(
            pool.write_ref(),
            resource_trade_no,
            err_code,
            err_msg,
        )
        .await
    }

    pub async fn mark_result_ack_sent(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_result_ack_sent(pool.write_ref(), resource_trade_no).await
    }

    pub async fn upsert_original_order_result_fact(
        pool: &ApiTransactionDbPool,
        input: NewApiResourceDelegation,
        result_status: ApiResourceDelegationResultStatus,
        fail_type: Option<i64>,
        result_payload: Option<&str>,
    ) -> Result<u64, crate::Error> {
        // This records a backend resource result that used the original
        // collect/withdraw trade no. It is a terminal result fact for ACK
        // retry, not a merchant-side placeholder task to execute.
        ApiResourceDelegationDao::upsert_original_order_result_fact(
            pool.write_ref(),
            input,
            result_status,
            fail_type,
            result_payload,
        )
        .await
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

    pub async fn mark_recover_retry_wait(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
        recover_status: ApiResourceDelegationRecoverStatus,
        next_retry_at: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_recover_retry_wait(
            pool.write_ref(),
            resource_trade_no,
            recover_status,
            next_retry_at,
        )
        .await
    }

    pub async fn reset_for_retry(
        pool: &ApiTransactionDbPool,
        resource_trade_no: &str,
        recover_status: ApiResourceDelegationRecoverStatus,
        next_retry_at: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::reset_for_retry(
            pool.write_ref(),
            resource_trade_no,
            recover_status,
            next_retry_at,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entities::{
            api_resource_delegation::{
                ApiResourceDelegationMode, ApiResourceDelegationOperationType,
                ApiResourceDelegationSource, ApiResourceDelegationStatus,
            },
            api_resource_type::ApiResourceType,
            api_trade_type::ApiTradeType,
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
    async fn resource_delegation_authorized_mode_persists_permission_id() {
        let pool = setup_api_transaction_pool("resource_delegation_authorized_mode").await;
        let input = NewApiResourceDelegation::platform_delegate_task(
            "uid_1",
            "rsc_auth_delegate",
            ApiTradeType::Collect,
            ApiResourceDelegationOperationType::Delegate,
            "tron",
            "T_authorized_owner",
            "T_receiver",
            ApiResourceType::Energy,
            "197",
            "14650",
        )
        .with_delegation_auth(ApiResourceDelegationMode::AuthorizedAddress, Some("2".to_string()));

        ApiResourceDelegationRepo::upsert(&pool, input).await.unwrap();

        let got = ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_auth_delegate")
            .await
            .unwrap();
        assert_eq!(got.delegation_mode, ApiResourceDelegationMode::AuthorizedAddress);
        assert_eq!(got.permission_id.as_deref(), Some("2"));
        assert_eq!(got.owner_address, "T_authorized_owner");
        assert_eq!(got.receiver_address, "T_receiver");
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
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_needs_ack",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Delegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "1",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_already_acked",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Delegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "1",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::local_delegate(
                "uid_1",
                "rsc_local_delegate",
                "origin_local",
                2,
                "owner",
                "receiver",
                "1",
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
        assert!(!trade_nos.contains(&"rsc_local_delegate".to_string()));
    }

    #[tokio::test]
    async fn platform_undelegation_task_ack_scan_finds_only_unacked_reclaim_tasks() {
        let pool = setup_api_transaction_pool("platform_undelegation_task_ack_scan").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_reclaim_needs_ack",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Undelegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "1",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_delegate_needs_ack",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Delegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "1",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_reclaim_acked",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Undelegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "1",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_reclaim_acked").await.unwrap();

        let rows =
            ApiResourceDelegationRepo::scan_need_task_ack_for_origin_type_source_and_operation(
                &pool,
                ApiTradeType::Collect as i64,
                ApiResourceDelegationSource::Platform,
                ApiResourceDelegationOperationType::Undelegate,
                100,
            )
            .await
            .unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();

        assert_eq!(trade_nos, vec!["rsc_reclaim_needs_ack".to_string()]);
    }

    #[tokio::test]
    async fn resource_delegation_execute_scan_finds_only_acked_unclaimed_tasks() {
        let pool = setup_api_transaction_pool("resource_delegation_execute_scan").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_unacked",
                "origin_unacked",
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
                "rsc_execute",
                "origin_execute",
                2,
                "owner",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_execute").await.unwrap();

        let rows = ApiResourceDelegationRepo::scan_can_execute(&pool, 100).await.unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();

        assert!(trade_nos.contains(&"rsc_execute".to_string()));
        assert!(!trade_nos.contains(&"rsc_unacked".to_string()));
    }

    #[tokio::test]
    async fn resource_delegation_local_execute_scan_finds_unacked_local_tasks() {
        let pool = setup_api_transaction_pool("resource_delegation_local_execute_scan").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::local_delegate(
                "uid_1",
                "rsc_local_execute",
                "origin_local_execute",
                2,
                "owner",
                "receiver",
                "100",
                "100",
            ),
        )
        .await
        .unwrap();

        let rows = ApiResourceDelegationRepo::scan_can_execute_for_origin_type_and_source(
            &pool,
            2,
            ApiResourceDelegationSource::Local,
            100,
        )
        .await
        .unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();

        assert_eq!(trade_nos, vec!["rsc_local_execute".to_string()]);
    }

    #[tokio::test]
    async fn resource_delegation_build_slot_claim_is_idempotent() {
        let pool = setup_api_transaction_pool("resource_delegation_build_claim").await;
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_claim",
                "origin_claim",
                2,
                "owner",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();

        assert_eq!(
            ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_claim").await.unwrap(),
            0
        );
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_claim").await.unwrap();
        assert_eq!(
            ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_claim").await.unwrap(),
            1
        );
        assert_eq!(
            ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_claim").await.unwrap(),
            0
        );

        let got =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_claim").await.unwrap();
        assert!(got.building_at.is_some());
        assert!(got.tx_hash.is_none());
        assert!(got.tx_status.is_none());
        assert!(got.result_status.is_none());
        assert!(got.result_received_at.is_none());
    }

    #[tokio::test]
    async fn resource_delegation_local_build_slot_claim_skips_task_ack_gate() {
        let pool = setup_api_transaction_pool("resource_delegation_local_build_claim").await;
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::local_delegate(
                "uid_1",
                "rsc_local_claim",
                "origin_local_claim",
                2,
                "owner",
                "receiver",
                "100",
                "100",
            ),
        )
        .await
        .unwrap();

        assert_eq!(
            ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_local_claim").await.unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn resource_delegation_broadcast_success_mark_is_idempotent() {
        let pool = setup_api_transaction_pool("resource_delegation_broadcast_success").await;
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_broadcast",
                "origin_broadcast",
                2,
                "owner",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_broadcast").await.unwrap();

        assert_eq!(
            ApiResourceDelegationRepo::mark_broadcast_success(
                &pool,
                "rsc_broadcast",
                "tx_before_claim",
            )
            .await
            .unwrap(),
            0
        );
        ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_broadcast").await.unwrap();
        assert_eq!(
            ApiResourceDelegationRepo::mark_broadcast_success(&pool, "rsc_broadcast", "tx_hash_1")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            ApiResourceDelegationRepo::mark_broadcast_success(&pool, "rsc_broadcast", "tx_hash_2")
                .await
                .unwrap(),
            0
        );

        let got = ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_broadcast")
            .await
            .unwrap();
        assert_eq!(got.tx_hash.as_deref(), Some("tx_hash_1"));
        assert_eq!(got.tx_status.as_deref(), Some("success"));
        assert!(got.result_status.is_none());
        assert!(got.result_received_at.is_none());
    }

    #[tokio::test]
    async fn resource_delegation_receipt_scan_and_mark_are_idempotent() {
        let pool = setup_api_transaction_pool("resource_delegation_receipt_upload").await;
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_receipt_ready",
                "origin_receipt_ready",
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
                "rsc_receipt_pending",
                "origin_receipt_pending",
                2,
                "owner",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();

        sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET task_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                building_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                tx_hash = 'tx_hash_ready',
                tx_status = 'success',
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no = 'rsc_receipt_ready'
            "#,
        )
        .execute(pool.as_ref())
        .await
        .unwrap();

        let rows =
            ApiResourceDelegationRepo::scan_need_tx_exec_receipt_upload(&pool, 100).await.unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();
        assert!(trade_nos.contains(&"rsc_receipt_ready".to_string()));
        assert!(!trade_nos.contains(&"rsc_receipt_pending".to_string()));

        assert_eq!(
            ApiResourceDelegationRepo::mark_tx_exec_receipt_uploaded(&pool, "rsc_receipt_ready")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            ApiResourceDelegationRepo::mark_tx_exec_receipt_uploaded(&pool, "rsc_receipt_ready")
                .await
                .unwrap(),
            0
        );

        let rows =
            ApiResourceDelegationRepo::scan_need_tx_exec_receipt_upload(&pool, 100).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn platform_undelegation_receipt_upload_scan_finds_successful_reclaim_tasks() {
        let pool = setup_api_transaction_pool("platform_undelegation_receipt_upload_scan").await;

        for trade_no in [
            "rsc_reclaim_receipt_ready",
            "rsc_withdraw_reclaim_receipt_ready",
            "rsc_reclaim_receipt_uploaded",
            "rsc_delegate_receipt_ready",
        ] {
            let operation_type = if trade_no == "rsc_delegate_receipt_ready" {
                ApiResourceDelegationOperationType::Delegate
            } else {
                ApiResourceDelegationOperationType::Undelegate
            };
            let origin_trade_type = if trade_no == "rsc_withdraw_reclaim_receipt_ready" {
                ApiTradeType::Withdraw
            } else {
                ApiTradeType::Collect
            };
            ApiResourceDelegationRepo::upsert(
                &pool,
                NewApiResourceDelegation::platform_delegate_task(
                    "uid_1",
                    trade_no,
                    origin_trade_type,
                    operation_type,
                    "tron",
                    "owner",
                    "receiver",
                    ApiResourceType::Energy,
                    "1",
                    "100",
                ),
            )
            .await
            .unwrap();
        }

        sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET task_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                building_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                tx_hash = resource_trade_no || '_tx_hash',
                tx_status = 'success',
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no IN (
                'rsc_reclaim_receipt_ready',
                'rsc_withdraw_reclaim_receipt_ready',
                'rsc_reclaim_receipt_uploaded',
                'rsc_delegate_receipt_ready'
            )
            "#,
        )
        .execute(pool.as_ref())
        .await
        .unwrap();

        assert_eq!(
            ApiResourceDelegationRepo::mark_tx_exec_receipt_uploaded_for_source_and_operation(
                &pool,
                "rsc_reclaim_receipt_uploaded",
                ApiResourceDelegationSource::Platform,
                ApiResourceDelegationOperationType::Undelegate,
            )
            .await
            .unwrap(),
            1
        );

        let rows =
            ApiResourceDelegationRepo::scan_need_tx_exec_receipt_upload_for_source_and_operation(
                &pool,
                ApiResourceDelegationSource::Platform,
                ApiResourceDelegationOperationType::Undelegate,
                100,
            )
            .await
            .unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();
        assert!(trade_nos.contains(&"rsc_reclaim_receipt_ready".to_string()));
        assert!(trade_nos.contains(&"rsc_withdraw_reclaim_receipt_ready".to_string()));
        assert!(!trade_nos.contains(&"rsc_reclaim_receipt_uploaded".to_string()));
        assert!(!trade_nos.contains(&"rsc_delegate_receipt_ready".to_string()));

        assert_eq!(
            ApiResourceDelegationRepo::mark_tx_exec_receipt_uploaded_for_source_and_operation(
                &pool,
                "rsc_reclaim_receipt_ready",
                ApiResourceDelegationSource::Platform,
                ApiResourceDelegationOperationType::Undelegate,
            )
            .await
            .unwrap(),
            1
        );
        let rows =
            ApiResourceDelegationRepo::scan_need_tx_exec_receipt_upload_for_source_and_operation(
                &pool,
                ApiResourceDelegationSource::Platform,
                ApiResourceDelegationOperationType::Undelegate,
                100,
            )
            .await
            .unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();
        assert!(!trade_nos.contains(&"rsc_reclaim_receipt_ready".to_string()));
        assert!(trade_nos.contains(&"rsc_withdraw_reclaim_receipt_ready".to_string()));
    }

    #[tokio::test]
    async fn platform_undelegation_result_ack_scan_finds_reclaim_results() {
        let pool = setup_api_transaction_pool("platform_undelegation_result_ack_scan").await;

        for trade_no in [
            "rsc_reclaim_result_ready",
            "rsc_withdraw_reclaim_result_ready",
            "rsc_reclaim_result_acked",
            "rsc_delegate_result_ready",
        ] {
            let operation_type = if trade_no == "rsc_delegate_result_ready" {
                ApiResourceDelegationOperationType::Delegate
            } else {
                ApiResourceDelegationOperationType::Undelegate
            };
            let origin_trade_type = if trade_no == "rsc_withdraw_reclaim_result_ready" {
                ApiTradeType::Withdraw
            } else {
                ApiTradeType::Collect
            };
            ApiResourceDelegationRepo::upsert(
                &pool,
                NewApiResourceDelegation::platform_delegate_task(
                    "uid_1",
                    trade_no,
                    origin_trade_type,
                    operation_type,
                    "tron",
                    "owner",
                    "receiver",
                    ApiResourceType::Energy,
                    "1",
                    "100",
                ),
            )
            .await
            .unwrap();
            ApiResourceDelegationRepo::mark_result_received(
                &pool,
                trade_no,
                ApiResourceDelegationResultStatus::Success,
                None,
                None,
                None,
                Some("payload"),
            )
            .await
            .unwrap();
        }

        assert_eq!(
            ApiResourceDelegationRepo::mark_result_ack_sent(&pool, "rsc_reclaim_result_acked")
                .await
                .unwrap(),
            1
        );

        let rows = ApiResourceDelegationRepo::scan_need_result_ack_for_source_and_operation(
            &pool,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Undelegate,
            100,
        )
        .await
        .unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();
        assert!(trade_nos.contains(&"rsc_reclaim_result_ready".to_string()));
        assert!(trade_nos.contains(&"rsc_withdraw_reclaim_result_ready".to_string()));
        assert!(!trade_nos.contains(&"rsc_reclaim_result_acked".to_string()));
        assert!(!trade_nos.contains(&"rsc_delegate_result_ready".to_string()));
    }

    #[tokio::test]
    async fn resource_delegation_failure_fact_enables_fail_receipt_scan() {
        let pool = setup_api_transaction_pool("resource_delegation_failure_receipt").await;
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_fail_receipt",
                "origin_fail_receipt",
                2,
                "owner",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();

        assert_eq!(
            ApiResourceDelegationRepo::mark_failed_if_unfinished(
                &pool,
                "rsc_fail_receipt",
                "ERR_6008",
                "sdk internal error",
            )
            .await
            .unwrap(),
            1
        );

        let got = ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_fail_receipt")
            .await
            .unwrap();
        assert_eq!(got.err_code.as_deref(), Some("ERR_6008"));
        assert_eq!(got.err_msg.as_deref(), Some("sdk internal error"));
        assert_eq!(got.tx_status.as_deref(), Some("fail"));
        assert_eq!(got.status, ApiResourceDelegationStatus::Fail);

        let rows =
            ApiResourceDelegationRepo::scan_need_tx_exec_receipt_upload(&pool, 100).await.unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();
        assert!(trade_nos.contains(&"rsc_fail_receipt".to_string()));
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
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_ack",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Delegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "1",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_no_result",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Delegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "1",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::local_delegate(
                "uid_1",
                "rsc_local_result",
                "origin_local_result",
                2,
                "owner",
                "receiver",
                "1",
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
            Some("{\"status\":true}"),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_local_failure_fact",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Delegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "1",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::mark_result_received(
            &pool,
            "rsc_local_failure_fact",
            ApiResourceDelegationResultStatus::Fail,
            Some(1),
            Some("ERR_6008"),
            Some("sdk internal error"),
            None,
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::mark_result_received(
            &pool,
            "rsc_local_result",
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
        assert!(!trade_nos.contains(&"rsc_local_failure_fact".to_string()));
        assert!(!trade_nos.contains(&"rsc_local_result".to_string()));

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

    #[tokio::test]
    async fn resource_delegation_original_order_result_fact_scans_for_result_ack_only() {
        let pool =
            setup_api_transaction_pool("resource_delegation_original_order_result_fact").await;
        let payload = r#"{"tradeNo":"C_origin","tradeType":"2","status":false}"#;

        ApiResourceDelegationRepo::upsert_original_order_result_fact(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "C_origin",
                "C_origin",
                ApiTradeType::Collect as i64,
                "",
                "",
                "0",
            ),
            ApiResourceDelegationResultStatus::Fail,
            Some(3),
            Some(payload),
        )
        .await
        .unwrap();

        let got =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "C_origin").await.unwrap();
        assert_eq!(got.status, ApiResourceDelegationStatus::Fail);
        assert_eq!(got.result_status, Some(ApiResourceDelegationResultStatus::Fail));
        assert_eq!(got.fail_type, Some(3));
        assert_eq!(got.result_payload.as_deref(), Some(payload));
        assert!(got.task_ack_sent_at.is_some());
        assert!(got.result_received_at.is_some());
        assert!(got.result_ack_sent_at.is_none());

        let task_ack_rows =
            ApiResourceDelegationRepo::scan_need_task_ack(&pool, 100).await.unwrap();
        assert!(!task_ack_rows.iter().any(|row| row.resource_trade_no == "C_origin"));

        let execute_rows = ApiResourceDelegationRepo::scan_can_execute(&pool, 100).await.unwrap();
        assert!(!execute_rows.iter().any(|row| row.resource_trade_no == "C_origin"));

        let result_ack_rows =
            ApiResourceDelegationRepo::scan_need_result_ack(&pool, 100).await.unwrap();
        assert!(result_ack_rows.iter().any(|row| row.resource_trade_no == "C_origin"));

        assert_eq!(
            ApiResourceDelegationRepo::mark_result_ack_sent(&pool, "C_origin").await.unwrap(),
            1
        );

        let result_ack_rows =
            ApiResourceDelegationRepo::scan_need_result_ack(&pool, 100).await.unwrap();
        assert!(!result_ack_rows.iter().any(|row| row.resource_trade_no == "C_origin"));
    }

    #[tokio::test]
    async fn resource_delegation_result_ack_retry_wait_throttles_scan() {
        let pool = setup_api_transaction_pool("resource_delegation_result_ack_retry").await;
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_ack_retry",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Delegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "1",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::mark_result_received(
            &pool,
            "rsc_ack_retry",
            ApiResourceDelegationResultStatus::Fail,
            Some(1),
            Some("ERR_6008"),
            Some("sdk internal error"),
            Some("{\"status\":false}"),
        )
        .await
        .unwrap();

        let rows = ApiResourceDelegationRepo::scan_need_result_ack(&pool, 100).await.unwrap();
        assert!(rows.iter().any(|row| row.resource_trade_no == "rsc_ack_retry"));

        let future = (chrono::Utc::now() + chrono::Duration::minutes(10))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        assert_eq!(
            ApiResourceDelegationRepo::mark_result_ack_retry_wait(&pool, "rsc_ack_retry", &future,)
                .await
                .unwrap(),
            1
        );

        let waiting = ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_ack_retry")
            .await
            .unwrap();
        assert_eq!(waiting.retry_count, 1);
        assert!(waiting.next_retry_at.is_some());

        let rows = ApiResourceDelegationRepo::scan_need_result_ack(&pool, 100).await.unwrap();
        assert!(!rows.iter().any(|row| row.resource_trade_no == "rsc_ack_retry"));

        let past = (chrono::Utc::now() - chrono::Duration::minutes(10))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        assert_eq!(
            ApiResourceDelegationRepo::mark_result_ack_retry_wait(&pool, "rsc_ack_retry", &past)
                .await
                .unwrap(),
            1
        );

        let rows = ApiResourceDelegationRepo::scan_need_result_ack(&pool, 100).await.unwrap();
        assert!(rows.iter().any(|row| row.resource_trade_no == "rsc_ack_retry"));
    }

    #[tokio::test]
    async fn local_undelegation_execute_and_recover_scans_respect_source_operation_and_retry() {
        let pool = setup_api_transaction_pool("resource_delegation_local_undelegation_scan").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::local_undelegate(
                "uid_1",
                "rsc_local_undelegate_ready",
                "origin_collect_1",
                crate::entities::api_trade_type::ApiTradeType::Collect as i64,
                "owner",
                "receiver",
                "5",
                "1000",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::local_undelegate(
                "uid_1",
                "rsc_local_undelegate_recover",
                "origin_collect_2",
                crate::entities::api_trade_type::ApiTradeType::Collect as i64,
                "owner",
                "receiver",
                "5",
                "1000",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_local_undelegate_recover")
            .await
            .unwrap();
        ApiResourceDelegationRepo::mark_broadcast_success(
            &pool,
            "rsc_local_undelegate_recover",
            "tx_hash_1",
        )
        .await
        .unwrap();

        let execute_rows =
            ApiResourceDelegationRepo::scan_can_execute_for_origin_type_source_and_operation(
                &pool,
                crate::entities::api_trade_type::ApiTradeType::Collect as i64,
                ApiResourceDelegationSource::Local,
                ApiResourceDelegationOperationType::Undelegate,
                100,
            )
            .await
            .unwrap();
        let execute_trade_nos: Vec<_> =
            execute_rows.into_iter().map(|row| row.resource_trade_no).collect();
        assert!(execute_trade_nos.contains(&"rsc_local_undelegate_ready".to_string()));
        assert!(!execute_trade_nos.contains(&"rsc_local_undelegate_recover".to_string()));

        let recover_rows =
            ApiResourceDelegationRepo::scan_can_recover_local_undelegation_for_origin_type(
                &pool,
                crate::entities::api_trade_type::ApiTradeType::Collect as i64,
                100,
            )
            .await
            .unwrap();
        let recover_trade_nos: Vec<_> =
            recover_rows.into_iter().map(|row| row.resource_trade_no).collect();
        assert!(recover_trade_nos.contains(&"rsc_local_undelegate_recover".to_string()));
        assert!(!recover_trade_nos.contains(&"rsc_local_undelegate_ready".to_string()));
    }

    #[tokio::test]
    async fn local_undelegation_retry_reset_returns_task_to_executable_state() {
        let pool = setup_api_transaction_pool("resource_delegation_local_undelegation_retry").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::local_undelegate(
                "uid_1",
                "rsc_local_undelegate_retry",
                "origin_collect_retry",
                crate::entities::api_trade_type::ApiTradeType::Collect as i64,
                "owner",
                "receiver",
                "5",
                "1000",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_local_undelegate_retry")
            .await
            .unwrap();
        ApiResourceDelegationRepo::mark_broadcast_success(
            &pool,
            "rsc_local_undelegate_retry",
            "tx_hash_retry",
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::reset_for_retry(
            &pool,
            "rsc_local_undelegate_retry",
            ApiResourceDelegationRecoverStatus::RetryRecover,
            "2099-01-01T00:00:00Z",
        )
        .await
        .unwrap();

        let persisted = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &pool,
            "rsc_local_undelegate_retry",
        )
        .await
        .unwrap();
        assert_eq!(persisted.tx_hash, None);
        assert_eq!(persisted.tx_status, None);
        assert_eq!(
            persisted.recover_status,
            Some(ApiResourceDelegationRecoverStatus::RetryRecover)
        );
        assert_eq!(persisted.retry_count, 1);
    }

    #[tokio::test]
    async fn platform_undelegation_reset_for_retry_releases_build_slot() {
        let pool =
            setup_api_transaction_pool("resource_delegation_platform_undelegation_retry").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_platform_undelegate_retry",
                crate::entities::api_trade_type::ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Undelegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "5",
                "1000",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_platform_undelegate_retry")
            .await
            .unwrap();
        ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_platform_undelegate_retry")
            .await
            .unwrap();
        ApiResourceDelegationRepo::mark_broadcast_success(
            &pool,
            "rsc_platform_undelegate_retry",
            "tx_hash_retry",
        )
        .await
        .unwrap();

        assert_eq!(
            ApiResourceDelegationRepo::reset_for_retry(
                &pool,
                "rsc_platform_undelegate_retry",
                ApiResourceDelegationRecoverStatus::RetryRecover,
                "2099-01-01T00:00:00Z",
            )
            .await
            .unwrap(),
            1
        );

        let persisted = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &pool,
            "rsc_platform_undelegate_retry",
        )
        .await
        .unwrap();
        assert!(persisted.building_at.is_none());
        assert_eq!(persisted.tx_hash, None);
        assert_eq!(persisted.tx_status, None);
        assert_eq!(
            persisted.recover_status,
            Some(ApiResourceDelegationRecoverStatus::RetryRecover)
        );
        assert_eq!(persisted.retry_count, 1);
    }

    #[tokio::test]
    async fn stale_platform_undelegation_build_slot_can_be_reclaimed() {
        let pool =
            setup_api_transaction_pool("resource_delegation_platform_undelegation_stale").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_platform_undelegate_stale",
                crate::entities::api_trade_type::ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Undelegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "5",
                "1000",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_platform_undelegate_stale")
            .await
            .unwrap();
        ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_platform_undelegate_stale")
            .await
            .unwrap();
        sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET building_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-10 minutes')
            WHERE resource_trade_no = ?
            "#,
        )
        .bind("rsc_platform_undelegate_stale")
        .execute(pool.write_ref())
        .await
        .unwrap();
        let rows =
            ApiResourceDelegationRepo::scan_can_execute_for_origin_type_source_and_operation(
                &pool,
                crate::entities::api_trade_type::ApiTradeType::Collect as i64,
                ApiResourceDelegationSource::Platform,
                ApiResourceDelegationOperationType::Undelegate,
                100,
            )
            .await
            .unwrap();
        assert!(rows.iter().any(|row| row.resource_trade_no == "rsc_platform_undelegate_stale"));

        assert_eq!(
            ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_platform_undelegate_stale")
                .await
                .unwrap(),
            1
        );
    }
}
