use crate::harness::{WorkerTestEnv, ensure_worker_env, next_unique_id, open_api_wallet_pool};
use sqlx;
use tempfile::TempDir;
use wallet_api::testkit::collect::{
    scan_collect_intent_labels_once, send_resource_result_ack_via_worker,
    upload_resource_tx_exec_receipt_via_worker,
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool, SqliteContext,
    entities::{
        api_collect::{ApiCollectEntity, ApiCollectStatus},
        api_resource_gate::{
            ApiResourceBlockReason, ApiResourceDependencyType, ApiResourceGateResult,
        },
        api_trade_type::ApiTradeType,
    },
    repositories::api_wallet::collect::ApiCollectRepo,
};

pub(super) struct CollectResourceGateFixture {
    trade_no: String,
    resource_trade_no: String,
}

impl CollectResourceGateFixture {
    pub(super) fn resource_scan_case(prefix: &str) -> Self {
        let id = next_unique_id();
        Self { trade_no: format!("C_RSC_SCAN_{id}"), resource_trade_no: format!("{prefix}_{id}") }
    }

    pub(super) fn origin_case(prefix: &str) -> Self {
        let trade_no = format!("{prefix}_{}", next_unique_id());
        Self { resource_trade_no: format!("rsc_delegate_{trade_no}"), trade_no }
    }
}

pub(super) struct LocalCollectResourceDb {
    _dir: TempDir,
    pool: ApiTransactionDbPool,
}

impl LocalCollectResourceDb {
    pub(super) async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx =
            SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_transaction.db"))
                .await
                .expect("init api_transaction.db");
        let pool = ctx.into_transaction_db_pool().expect("transaction pool");
        Self { _dir: dir, pool }
    }

    pub(super) async fn given_failed_delegation_ready_for_receipt_scan(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        insert_failed_delegation_ready_for_receipt_scan(&self.pool, &fixture.resource_trade_no)
            .await;
    }

    pub(super) async fn when_collect_scanner_runs(&self) -> Vec<String> {
        scan_collect_intent_labels_once(self.pool.clone())
            .await
            .expect("scanner round should succeed")
    }

    pub(super) fn then_scanner_emits_resource_receipt_upload(&self, labels: Vec<String>) {
        assert!(
            labels.iter().any(|label| label == "UploadResourceTxExecReceipt"),
            "failed resource delegation should emit UploadResourceTxExecReceipt"
        );
    }
}

pub(super) struct CollectResourceGateScenario {
    collect_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
}

impl CollectResourceGateScenario {
    pub(super) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let collect_pool = open_collect_pool(env).await;
        let core_pool = open_api_wallet_pool(&env.db_dir).await;

        Self { collect_pool, core_pool }
    }

    pub(super) async fn given_blocked_collect(&self, fixture: &CollectResourceGateFixture) {
        seed_blocked_collect(&self.collect_pool, &fixture.trade_no).await;
    }

    pub(super) async fn given_successful_collect_resource_result(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        seed_resource_result(
            &self.collect_pool,
            &fixture.trade_no,
            &fixture.resource_trade_no,
            ApiTradeType::Collect,
            true,
        )
        .await;
    }

    pub(super) async fn given_failed_collect_resource_result(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        seed_resource_result(
            &self.collect_pool,
            &fixture.trade_no,
            &fixture.resource_trade_no,
            ApiTradeType::Collect,
            false,
        )
        .await;
    }

    pub(super) async fn given_successful_withdraw_origin_resource_result(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        seed_resource_result(
            &self.collect_pool,
            "W_ORIGIN_SKIP",
            &fixture.resource_trade_no,
            ApiTradeType::Withdraw,
            true,
        )
        .await;
    }

    pub(super) async fn given_failed_collect_resource_receipt(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        seed_failed_resource_receipt_row(
            &self.collect_pool,
            Some(&fixture.trade_no),
            &fixture.resource_trade_no,
        )
        .await;
    }

    pub(super) async fn given_failed_resource_receipt_without_origin_trade(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        seed_failed_resource_receipt_row(&self.collect_pool, None, &fixture.resource_trade_no)
            .await;
    }

    pub(super) async fn when_resource_result_ack_is_sent(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        send_resource_result_ack_via_worker(
            self.collect_pool.clone(),
            self.core_pool.clone(),
            &fixture.resource_trade_no,
        )
        .await
        .expect("send resource result ack");
    }

    pub(super) async fn when_resource_receipt_upload_is_sent(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        upload_resource_tx_exec_receipt_via_worker(
            self.collect_pool.clone(),
            self.core_pool.clone(),
            &fixture.resource_trade_no,
        )
        .await
        .expect("upload resource tx exec receipt");
    }

    pub(super) async fn then_origin_collect_gate_is_released(
        &self,
        fixture: &CollectResourceGateFixture,
        expected_result: ApiResourceGateResult,
    ) {
        let collect = self.load_collect(&fixture.trade_no).await;
        assert!(collect.resource_gate_released_at.is_some());
        assert_eq!(collect.resource_gate_result, Some(expected_result));
    }

    pub(super) async fn then_origin_collect_gate_is_not_released(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        let collect = self.load_collect(&fixture.trade_no).await;
        assert!(collect.resource_gate_released_at.is_none());
        assert!(collect.resource_gate_result.is_none());
    }

    pub(super) async fn then_collect_can_build(&self, fixture: &CollectResourceGateFixture) {
        assert!(
            self.is_collect_build_candidate(&fixture.trade_no).await,
            "released collect should be eligible for BuildTx"
        );
    }

    pub(super) async fn then_collect_cannot_build(&self, fixture: &CollectResourceGateFixture) {
        assert!(
            !self.is_collect_build_candidate(&fixture.trade_no).await,
            "blocked collect should not be eligible for BuildTx before local delegation fallback"
        );
    }

    pub(super) async fn then_collect_still_waits_for_platform_delegate(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        let collect = self.load_collect(&fixture.trade_no).await;
        assert!(collect.resource_gate_released_at.is_none());
        assert_eq!(
            collect.resource_dependency_trade_no.as_deref(),
            Some(fixture.resource_trade_no.as_str())
        );
        assert_eq!(
            collect.resource_dependency_type,
            Some(ApiResourceDependencyType::PlatformDelegate)
        );
        assert_eq!(
            collect.resource_block_reason,
            Some(ApiResourceBlockReason::NeedPlatformDelegate)
        );
    }

    async fn load_collect(&self, trade_no: &str) -> ApiCollectEntity {
        ApiCollectRepo::get_api_collect_by_trade_no(&self.collect_pool, trade_no)
            .await
            .expect("load collect")
    }

    async fn is_collect_build_candidate(&self, trade_no: &str) -> bool {
        ApiCollectRepo::scan_can_build(&self.collect_pool, 10_000)
            .await
            .expect("scan collect build candidates")
            .iter()
            .any(|collect| collect.trade_no == trade_no)
    }
}

async fn open_collect_pool(env: &WorkerTestEnv) -> ApiTransactionDbPool {
    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    collect_pool_ctx.into_transaction_db_pool().expect("transaction pool")
}

async fn seed_blocked_collect(collect_pool: &ApiTransactionDbPool, trade_no: &str) {
    ApiCollectRepo::upsert_api_collect(
        collect_pool,
        "uid",
        "collect",
        "from",
        "to",
        "1.12",
        "digest",
        "tron",
        None,
        "TRX",
        trade_no,
        ApiTradeType::Collect as u8,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

    sqlx::query(
        "UPDATE api_collect SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE trade_no = ?",
    )
    .bind(trade_no)
    .execute(collect_pool.as_ref())
    .await
    .expect("seed order ack");

    ApiCollectRepo::mark_resource_blocked(
        collect_pool,
        trade_no,
        ApiResourceBlockReason::NeedPlatformDelegate,
        Some(&format!("rsc_delegate_{trade_no}")),
        Some(ApiResourceDependencyType::PlatformDelegate),
    )
    .await
    .expect("seed blocked collect");
}

async fn seed_resource_result(
    collect_pool: &ApiTransactionDbPool,
    trade_no: &str,
    resource_trade_no: &str,
    origin_trade_type: ApiTradeType,
    success: bool,
) {
    let (tx_status, err_code, err_msg, result_status, result_payload) = if success {
        ("success", None, None, 1_i64, r#"{"status":true}"#)
    } else {
        ("fail", Some("ERR_6008"), Some("delegate failed"), 2_i64, r#"{"status":false}"#)
    };

    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            tx_hash, tx_status, err_code, err_msg, result_status, result_received_at, result_payload,
            created_at, updated_at
        ) VALUES (
            'uid', 1, 1, ?, ?,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            'tx_hash_collect_resource', ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), ?,
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(trade_no)
    .bind(origin_trade_type as i64)
    .bind(resource_trade_no)
    .bind(tx_status)
    .bind(err_code)
    .bind(err_msg)
    .bind(result_status)
    .bind(result_payload)
    .execute(collect_pool.as_ref())
    .await
    .expect("seed resource delegation row for result ack");
}

async fn seed_failed_resource_receipt_row(
    collect_pool: &ApiTransactionDbPool,
    trade_no: Option<&str>,
    resource_trade_no: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            err_code, err_msg, tx_status,
            created_at, updated_at
        ) VALUES (
            'uid', 1, 1, ?, ?,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            'delegate_failed', 'delegate failed', 'fail',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(trade_no)
    .bind(ApiTradeType::Collect as i64)
    .bind(resource_trade_no)
    .execute(collect_pool.as_ref())
    .await
    .expect("seed failed collect delegation row");
}

async fn insert_failed_delegation_ready_for_receipt_scan(
    collect_pool: &ApiTransactionDbPool,
    resource_trade_no: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            task_ack_sent_at, building_at, tx_status, err_code, err_msg,
            created_at, updated_at
        ) VALUES (
            'uid', 1, 1, NULL, 2,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            'fail', 'ERR_6008', 'sdk internal error',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(resource_trade_no)
    .execute(collect_pool.as_ref())
    .await
    .expect("seed failed resource delegation row");
}
