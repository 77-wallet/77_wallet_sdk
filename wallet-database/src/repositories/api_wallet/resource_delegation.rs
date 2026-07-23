use crate::{
    ApiResourceDbPool,
    dao::api_resource_delegation::ApiResourceDelegationDao,
    entities::api_resource_delegation::{
        ApiResourceDelegationEntity, ApiResourceDelegationOperationType,
        ApiResourceDelegationResultStatus, ApiResourceDelegationSource, NewApiResourceDelegation,
    },
};

pub struct ApiResourceDelegationRepo;

impl ApiResourceDelegationRepo {
    pub async fn upsert(
        pool: &ApiResourceDbPool,
        input: NewApiResourceDelegation,
    ) -> Result<(), crate::Error> {
        ApiResourceDelegationDao::upsert(pool.write_ref(), input).await
    }

    pub async fn get_by_resource_trade_no(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
    ) -> Result<ApiResourceDelegationEntity, crate::Error> {
        ApiResourceDelegationDao::get_by_resource_trade_no(pool.read_ref(), resource_trade_no).await
    }

    pub async fn find_by_resource_trade_no(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
    ) -> Result<Option<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::find_by_resource_trade_no(pool.read_ref(), resource_trade_no)
            .await
    }

    pub async fn list_by_origin_trade_no(
        pool: &ApiResourceDbPool,
        origin_trade_no: &str,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::list_by_origin_trade_no(pool.read_ref(), origin_trade_no).await
    }

    pub async fn mark_task_ack_sent(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_task_ack_sent(pool.write_ref(), resource_trade_no).await
    }

    pub async fn mark_task_ack_attempted(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_task_ack_attempted(pool.write_ref(), resource_trade_no).await
    }

    pub async fn scan_need_task_ack(
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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

    pub async fn mark_result_ack_attempted(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_result_ack_attempted(pool.write_ref(), resource_trade_no)
            .await
    }

    pub async fn scan_need_result_ack(
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::claim_build_slot(pool.write_ref(), resource_trade_no).await
    }

    pub async fn mark_broadcast_pending(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
        tx_hash: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_broadcast_pending(
            pool.write_ref(),
            resource_trade_no,
            tx_hash,
        )
        .await
    }

    pub async fn mark_broadcast_success(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
        tx_hash: &str,
    ) -> Result<u64, crate::Error> {
        Self::mark_broadcast_pending(pool, resource_trade_no, tx_hash).await
    }

    pub async fn mark_chain_success(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
        transaction_time: Option<&str>,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_chain_success(
            pool.write_ref(),
            resource_trade_no,
            transaction_time,
        )
        .await
    }

    pub async fn mark_broadcast_uncertain_attempt(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_broadcast_uncertain_attempt(
            pool.write_ref(),
            resource_trade_no,
        )
        .await
    }

    pub async fn mark_broadcast_uncertain_rebroadcast_attempted(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_broadcast_uncertain_rebroadcast_attempted(
            pool.write_ref(),
            resource_trade_no,
        )
        .await
    }

    pub async fn mark_broadcast_uncertain_reconciled(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_broadcast_uncertain_reconciled(
            pool.write_ref(),
            resource_trade_no,
        )
        .await
    }

    pub async fn scan_need_tx_exec_receipt_upload_for_origin_type(
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_tx_exec_receipt_uploaded(pool.write_ref(), resource_trade_no)
            .await
    }

    pub async fn mark_tx_exec_receipt_uploaded_for_source_and_operation(
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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

    pub async fn scan_failed_for_origin_type_source_and_operation(
        pool: &ApiResourceDbPool,
        origin_trade_type: i64,
        source: ApiResourceDelegationSource,
        operation_type: ApiResourceDelegationOperationType,
        limit: usize,
    ) -> Result<Vec<ApiResourceDelegationEntity>, crate::Error> {
        ApiResourceDelegationDao::scan_failed_by_origin_type_source_and_operation(
            pool.read_ref(),
            origin_trade_type,
            source,
            operation_type,
            limit,
        )
        .await
    }

    pub async fn mark_result_ack_sent(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_result_ack_sent(pool.write_ref(), resource_trade_no).await
    }

    pub async fn upsert_original_order_result_fact(
        pool: &ApiResourceDbPool,
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
        pool: &ApiResourceDbPool,
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

    pub async fn mark_backend_result_received(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
        result_status: ApiResourceDelegationResultStatus,
        fail_type: Option<i64>,
        err_code: Option<&str>,
        err_msg: Option<&str>,
        result_payload: Option<&str>,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::mark_backend_result_received(
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

    pub async fn reset_for_rebuild(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiResourceDelegationDao::reset_for_rebuild(pool.write_ref(), resource_trade_no).await
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
        repositories::test_helper::setup_api_resource_pool,
    };

    #[tokio::test]
    async fn resource_delegation_upsert_is_idempotent_and_preserves_source_boundary() {
        let pool = setup_api_resource_pool("resource_delegation_upsert").await;
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
        let pool = setup_api_resource_pool("resource_delegation_authorized_mode").await;
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
        let pool = setup_api_resource_pool("resource_delegation_ack").await;
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
        let pool = setup_api_resource_pool("resource_delegation_task_ack_scan").await;

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
        let pool = setup_api_resource_pool("platform_undelegation_task_ack_scan").await;

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
    async fn resource_delegation_task_ack_attempt_tracks_send_count() {
        let pool = setup_api_resource_pool("resource_delegation_task_ack_retry_wait").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_task_ack_retry",
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

        assert_eq!(
            ApiResourceDelegationRepo::mark_task_ack_attempted(&pool, "rsc_task_ack_retry")
                .await
                .unwrap(),
            1
        );

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
        assert!(rows.iter().any(|row| row.resource_trade_no == "rsc_task_ack_retry"));

        let persisted =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_task_ack_retry")
                .await
                .unwrap();
        assert_eq!(persisted.task_ack_send_count, 1);
    }

    #[tokio::test]
    async fn resource_delegation_task_ack_attempts_remain_scannable_until_sent() {
        let pool = setup_api_resource_pool("resource_delegation_task_ack_attempts").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_task_ack_attempts",
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

        assert_eq!(
            ApiResourceDelegationRepo::mark_task_ack_attempted(&pool, "rsc_task_ack_attempts")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            ApiResourceDelegationRepo::mark_task_ack_attempted(&pool, "rsc_task_ack_attempts")
                .await
                .unwrap(),
            1
        );

        let rows = ApiResourceDelegationRepo::scan_need_task_ack(&pool, 100).await.unwrap();
        assert!(rows.iter().any(|row| row.resource_trade_no == "rsc_task_ack_attempts"));

        assert_eq!(
            ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_task_ack_attempts")
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn resource_delegation_execute_scan_finds_only_acked_unclaimed_tasks() {
        let pool = setup_api_resource_pool("resource_delegation_execute_scan").await;

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
        let pool = setup_api_resource_pool("resource_delegation_local_execute_scan").await;

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
        let pool = setup_api_resource_pool("resource_delegation_build_claim").await;
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
        assert!(got.result_status.is_none());
        assert!(got.finished_at.is_none());
    }

    #[tokio::test]
    async fn resource_delegation_task_ack_releases_build_claim_cooldown() {
        let pool = setup_api_resource_pool("resource_delegation_ack_releases_claim").await;
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_ack_execute",
                "origin_ack_execute",
                2,
                "owner",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();

        assert_eq!(
            ApiResourceDelegationRepo::mark_task_ack_attempted(&pool, "rsc_ack_execute")
                .await
                .unwrap(),
            1
        );
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_ack_execute").await.unwrap();

        let acked = ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_ack_execute")
            .await
            .unwrap();
        assert_eq!(acked.task_ack_send_count, 1);
        assert_eq!(
            ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_ack_execute").await.unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn resource_delegation_rebuild_reset_releases_build_claim() {
        let pool = setup_api_resource_pool("resource_delegation_retry_blocks_build").await;
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "rsc_retry_execute",
                "origin_retry_execute",
                2,
                "owner",
                "receiver",
                "100",
            ),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_retry_execute").await.unwrap();
        ApiResourceDelegationRepo::reset_for_rebuild(&pool, "rsc_retry_execute").await.unwrap();

        assert_eq!(
            ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_retry_execute").await.unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn resource_delegation_local_build_slot_claim_skips_task_ack_gate() {
        let pool = setup_api_resource_pool("resource_delegation_local_build_claim").await;
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
    async fn resource_delegation_broadcast_mark_is_pending_until_chain_success() {
        let pool = setup_api_resource_pool("resource_delegation_broadcast_success").await;
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
        assert!(got.last_broadcast_at.is_some());
        assert!(got.chain_success_at.is_none());
        assert!(got.transaction_time.is_none());
        assert!(got.result_status.is_none());
        assert!(got.finished_at.is_none());
        assert!(got.broadcast_uncertain_since_at.is_none());
        assert_eq!(got.broadcast_uncertain_retry_count, 0);
        assert_eq!(got.broadcast_uncertain_rebroadcast_count, 0);

        let ready = ApiResourceDelegationRepo::scan_need_tx_exec_receipt_upload_for_origin_type(
            &pool, 2, 10,
        )
        .await
        .unwrap();
        assert!(ready.is_empty());

        assert_eq!(
            ApiResourceDelegationRepo::mark_broadcast_uncertain_attempt(&pool, "rsc_broadcast")
                .await
                .unwrap(),
            1
        );
        let got = ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_broadcast")
            .await
            .unwrap();
        assert!(got.broadcast_uncertain_since_at.is_some());
        assert_eq!(got.broadcast_uncertain_retry_count, 1);
        assert!(got.broadcast_uncertain_last_checked_at.is_some());
        assert!(got.broadcast_uncertain_reconciled_at.is_none());
        assert_eq!(
            ApiResourceDelegationRepo::mark_broadcast_uncertain_rebroadcast_attempted(
                &pool,
                "rsc_broadcast",
            )
            .await
            .unwrap(),
            1
        );
        let got = ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_broadcast")
            .await
            .unwrap();
        assert_eq!(got.broadcast_uncertain_rebroadcast_count, 1);

        assert_eq!(
            ApiResourceDelegationRepo::mark_chain_success(&pool, "rsc_broadcast", None)
                .await
                .unwrap(),
            1
        );
        let got = ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_broadcast")
            .await
            .unwrap();
        assert!(got.chain_success_at.is_some());
        assert!(got.transaction_time.is_some());
        assert!(got.broadcast_uncertain_since_at.is_none());
        assert_eq!(got.broadcast_uncertain_retry_count, 0);
        assert!(got.broadcast_uncertain_last_checked_at.is_none());
        assert!(got.broadcast_uncertain_reconciled_at.is_some());
        assert_eq!(got.broadcast_uncertain_rebroadcast_count, 0);
    }

    #[tokio::test]
    async fn resource_delegation_receipt_scan_and_mark_are_idempotent() {
        let pool = setup_api_resource_pool("resource_delegation_receipt_upload").await;
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
                last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                chain_success_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
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
        let pool = setup_api_resource_pool("platform_undelegation_receipt_upload_scan").await;

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
                last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                chain_success_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
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
        let pool = setup_api_resource_pool("platform_undelegation_result_ack_scan").await;

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
            ApiResourceDelegationRepo::mark_backend_result_received(
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
    async fn platform_undelegation_result_ack_requires_tx_res_received_at() {
        let pool = setup_api_resource_pool("platform_undelegation_result_ack_tx_res_gate").await;

        for trade_no in ["rsc_reclaim_recovered", "rsc_reclaim_backend_result"] {
            ApiResourceDelegationRepo::upsert(
                &pool,
                NewApiResourceDelegation::platform_delegate_task(
                    "uid_1",
                    trade_no,
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
        }

        ApiResourceDelegationRepo::mark_result_received(
            &pool,
            "rsc_reclaim_recovered",
            ApiResourceDelegationResultStatus::Success,
            None,
            None,
            None,
            Some("platform_undelegation_recovered:tx_hash"),
        )
        .await
        .unwrap();
        ApiResourceDelegationRepo::mark_backend_result_received(
            &pool,
            "rsc_reclaim_backend_result",
            ApiResourceDelegationResultStatus::Success,
            None,
            None,
            None,
            Some(r#"{"tradeNo":"rsc_reclaim_backend_result","status":true}"#),
        )
        .await
        .unwrap();

        let recovered =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_reclaim_recovered")
                .await
                .unwrap();
        assert!(recovered.finished_at.is_some());
        assert!(recovered.tx_res_received_at.is_none());

        let backend_result = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &pool,
            "rsc_reclaim_backend_result",
        )
        .await
        .unwrap();
        assert!(backend_result.finished_at.is_some());
        assert!(backend_result.tx_res_received_at.is_some());

        let rows = ApiResourceDelegationRepo::scan_need_result_ack_for_source_and_operation(
            &pool,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Undelegate,
            100,
        )
        .await
        .unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();
        assert!(!trade_nos.contains(&"rsc_reclaim_recovered".to_string()));
        assert!(trade_nos.contains(&"rsc_reclaim_backend_result".to_string()));

        assert_eq!(
            ApiResourceDelegationRepo::mark_result_ack_sent(&pool, "rsc_reclaim_recovered")
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            ApiResourceDelegationRepo::mark_result_ack_sent(&pool, "rsc_reclaim_backend_result")
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn resource_delegation_failure_fact_enables_fail_receipt_scan() {
        let pool = setup_api_resource_pool("resource_delegation_failure_receipt").await;
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
        assert_eq!(got.status, ApiResourceDelegationStatus::Fail);

        let rows =
            ApiResourceDelegationRepo::scan_need_tx_exec_receipt_upload(&pool, 100).await.unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();
        assert!(trade_nos.contains(&"rsc_fail_receipt".to_string()));
    }

    #[tokio::test]
    async fn resource_delegation_result_records_success_and_failure_facts() {
        let pool = setup_api_resource_pool("resource_delegation_result").await;

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
        assert!(success.finished_at.is_some());
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
        let pool = setup_api_resource_pool("resource_delegation_result_ack").await;
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

        ApiResourceDelegationRepo::mark_backend_result_received(
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

        let rows = ApiResourceDelegationRepo::scan_need_result_ack_for_source_and_operation(
            &pool,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Delegate,
            100,
        )
        .await
        .unwrap();
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

        let rows = ApiResourceDelegationRepo::scan_need_result_ack_for_source_and_operation(
            &pool,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Delegate,
            100,
        )
        .await
        .unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn resource_delegation_original_order_result_fact_scans_for_result_ack_only() {
        let pool = setup_api_resource_pool("resource_delegation_original_order_result_fact").await;
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
        assert!(got.finished_at.is_some());
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
    async fn resource_delegation_origin_result_ack_scan_skips_platform_tasks() {
        let pool =
            setup_api_resource_pool("resource_delegation_origin_ack_skips_platform_task").await;
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "WD_platform_task",
                ApiTradeType::Withdraw,
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
        ApiResourceDelegationRepo::mark_backend_result_received(
            &pool,
            "WD_platform_task",
            ApiResourceDelegationResultStatus::Success,
            Some(0),
            None,
            None,
            Some(r#"{"tradeNo":"WD_platform_task","status":true}"#),
        )
        .await
        .unwrap();

        ApiResourceDelegationRepo::upsert_original_order_result_fact(
            &pool,
            NewApiResourceDelegation::platform_delegate(
                "uid_1",
                "W_origin_result",
                "W_origin_result",
                ApiTradeType::Withdraw as i64,
                "",
                "",
                "0",
            ),
            ApiResourceDelegationResultStatus::Success,
            None,
            Some(r#"{"tradeNo":"W_origin_result","status":true}"#),
        )
        .await
        .unwrap();

        let rows = ApiResourceDelegationRepo::scan_need_result_ack_for_origin_type(
            &pool,
            ApiTradeType::Withdraw as i64,
            100,
        )
        .await
        .unwrap();
        let trade_nos: Vec<_> = rows.into_iter().map(|row| row.resource_trade_no).collect();

        assert_eq!(trade_nos, vec!["W_origin_result".to_string()]);
    }

    #[tokio::test]
    async fn resource_delegation_result_ack_attempt_tracks_send_count() {
        let pool = setup_api_resource_pool("resource_delegation_result_ack_retry").await;
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
        ApiResourceDelegationRepo::mark_backend_result_received(
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

        let rows = ApiResourceDelegationRepo::scan_need_result_ack_for_source_and_operation(
            &pool,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Delegate,
            100,
        )
        .await
        .unwrap();
        assert!(rows.iter().any(|row| row.resource_trade_no == "rsc_ack_retry"));

        assert_eq!(
            ApiResourceDelegationRepo::mark_result_ack_attempted(&pool, "rsc_ack_retry")
                .await
                .unwrap(),
            1
        );

        let waiting = ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_ack_retry")
            .await
            .unwrap();
        assert_eq!(waiting.result_ack_send_count, 1);

        let rows = ApiResourceDelegationRepo::scan_need_result_ack_for_source_and_operation(
            &pool,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Delegate,
            100,
        )
        .await
        .unwrap();
        assert!(rows.iter().any(|row| row.resource_trade_no == "rsc_ack_retry"));
    }

    #[tokio::test]
    async fn resource_delegation_result_ack_attempts_remain_scannable_until_sent() {
        let pool = setup_api_resource_pool("resource_delegation_result_ack_attempts").await;
        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_result_ack_attempts",
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
        ApiResourceDelegationRepo::mark_backend_result_received(
            &pool,
            "rsc_result_ack_attempts",
            ApiResourceDelegationResultStatus::Success,
            None,
            None,
            None,
            Some("{\"status\":true}"),
        )
        .await
        .unwrap();

        assert_eq!(
            ApiResourceDelegationRepo::mark_result_ack_attempted(&pool, "rsc_result_ack_attempts")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            ApiResourceDelegationRepo::mark_result_ack_attempted(&pool, "rsc_result_ack_attempts")
                .await
                .unwrap(),
            1
        );

        let rows = ApiResourceDelegationRepo::scan_need_result_ack_for_source_and_operation(
            &pool,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Delegate,
            100,
        )
        .await
        .unwrap();
        assert!(rows.iter().any(|row| row.resource_trade_no == "rsc_result_ack_attempts"));

        assert_eq!(
            ApiResourceDelegationRepo::mark_result_ack_sent(&pool, "rsc_result_ack_attempts")
                .await
                .unwrap(),
            1
        );
        let acked =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_result_ack_attempts")
                .await
                .unwrap();
        assert_eq!(acked.result_ack_send_count, 2);
    }

    #[tokio::test]
    async fn local_undelegation_scans_respect_source_operation_and_chain_facts() {
        let pool = setup_api_resource_pool("resource_delegation_local_undelegation_scan").await;

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
    async fn local_undelegation_rebuild_reset_returns_task_to_executable_state() {
        let pool = setup_api_resource_pool("resource_delegation_local_undelegation_retry").await;

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
        ApiResourceDelegationRepo::reset_for_rebuild(&pool, "rsc_local_undelegate_retry")
            .await
            .unwrap();

        let persisted = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &pool,
            "rsc_local_undelegate_retry",
        )
        .await
        .unwrap();
        assert_eq!(persisted.tx_hash, None);
    }

    #[tokio::test]
    async fn platform_undelegation_reset_for_rebuild_releases_build_slot() {
        let pool = setup_api_resource_pool("resource_delegation_platform_undelegation_retry").await;

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
            ApiResourceDelegationRepo::reset_for_rebuild(&pool, "rsc_platform_undelegate_retry")
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
    }

    #[tokio::test]
    async fn platform_delegate_reset_for_rebuild_releases_build_slot() {
        let pool = setup_api_resource_pool("resource_delegation_platform_delegate_retry").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_platform_delegate_retry",
                crate::entities::api_trade_type::ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Delegate,
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
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_platform_delegate_retry")
            .await
            .unwrap();
        ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_platform_delegate_retry")
            .await
            .unwrap();

        assert_eq!(
            ApiResourceDelegationRepo::reset_for_rebuild(&pool, "rsc_platform_delegate_retry")
                .await
                .unwrap(),
            1
        );

        let persisted = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &pool,
            "rsc_platform_delegate_retry",
        )
        .await
        .unwrap();
        assert!(persisted.building_at.is_none());
        assert_eq!(persisted.tx_hash, None);
    }

    #[tokio::test]
    async fn stale_platform_undelegation_build_slot_can_be_reclaimed() {
        let pool = setup_api_resource_pool("resource_delegation_platform_undelegation_stale").await;

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

    #[tokio::test]
    async fn stale_platform_delegation_build_slot_can_be_reclaimed() {
        let pool = setup_api_resource_pool("resource_delegation_platform_delegation_stale").await;

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid_1",
                "rsc_platform_delegate_stale",
                crate::entities::api_trade_type::ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Delegate,
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
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_platform_delegate_stale")
            .await
            .unwrap();
        ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_platform_delegate_stale")
            .await
            .unwrap();
        sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET building_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-10 minutes')
            WHERE resource_trade_no = ?
            "#,
        )
        .bind("rsc_platform_delegate_stale")
        .execute(pool.write_ref())
        .await
        .unwrap();
        let rows =
            ApiResourceDelegationRepo::scan_can_execute_for_origin_type_source_and_operation(
                &pool,
                crate::entities::api_trade_type::ApiTradeType::Collect as i64,
                ApiResourceDelegationSource::Platform,
                ApiResourceDelegationOperationType::Delegate,
                100,
            )
            .await
            .unwrap();
        assert!(rows.iter().any(|row| row.resource_trade_no == "rsc_platform_delegate_stale"));

        assert_eq!(
            ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_platform_delegate_stale")
                .await
                .unwrap(),
            1
        );
    }
}
