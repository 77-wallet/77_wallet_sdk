use crate::harness::{WorkerTestEnv, ensure_worker_env, next_unique_id, open_api_wallet_pool};
use serial_test::serial;
use sqlx;
use tempfile::TempDir;
use wallet_api::test::collect::{
    scan_collect_intent_labels_once, send_resource_result_ack_via_worker,
    upload_resource_tx_exec_receipt_via_worker,
};
use wallet_database::{
    ApiTransactionDbPool, SqliteContext,
    entities::{
        api_collect::ApiCollectStatus,
        api_resource_gate::{
            ApiResourceBlockReason, ApiResourceDependencyType, ApiResourceGateResult,
        },
        api_trade_type::ApiTradeType,
    },
    repositories::api_wallet::collect::ApiCollectRepo,
};

struct LocalCollectDb {
    _dir: TempDir,
    pool: ApiTransactionDbPool,
}

impl LocalCollectDb {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx =
            SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_transaction.db"))
                .await
                .expect("init api_transaction.db");
        let pool = ctx.into_transaction_db_pool().expect("transaction pool");
        Self { _dir: dir, pool }
    }
}

async fn open_collect_pool(env: &WorkerTestEnv) -> ApiTransactionDbPool {
    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    collect_pool_ctx.into_transaction_db_pool().expect("transaction pool")
}

async fn is_collect_build_candidate(collect_pool: &ApiTransactionDbPool, trade_no: &str) -> bool {
    ApiCollectRepo::scan_can_build(collect_pool, 10_000)
        .await
        .expect("scan collect build candidates")
        .iter()
        .any(|collect| collect.trade_no == trade_no)
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

#[tokio::test]
async fn collect_scanner_emits_resource_receipt_upload_for_failed_delegation() {
    let db = LocalCollectDb::new().await;
    let resource_trade_no = format!("RSC_FAIL_RECEIPT_SCAN_{}", next_unique_id());

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
    .bind(&resource_trade_no)
    .execute(db.pool.as_ref())
    .await
    .expect("seed failed resource delegation row");

    let labels = scan_collect_intent_labels_once(db.pool.clone())
        .await
        .expect("scanner round should succeed");

    assert!(
        labels.iter().any(|label| label == "UploadResourceTxExecReceipt"),
        "failed resource delegation should emit UploadResourceTxExecReceipt"
    );
}

#[tokio::test]
#[serial]
async fn collect_resource_result_ack_releases_origin_collect_gate() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let collect_pool = open_collect_pool(env).await;
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    let trade_no = format!("C_RSC_RELEASE_{}", next_unique_id());
    let resource_trade_no = format!("rsc_delegate_{trade_no}");

    seed_blocked_collect(&collect_pool, &trade_no).await;
    seed_resource_result(&collect_pool, &trade_no, &resource_trade_no, ApiTradeType::Collect, true)
        .await;

    send_resource_result_ack_via_worker(collect_pool.clone(), core_pool, &resource_trade_no)
        .await
        .expect("send resource result ack");

    let collect = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect");
    assert!(collect.resource_gate_released_at.is_some());
    assert_eq!(
        collect.resource_gate_result,
        Some(ApiResourceGateResult::ResourceDelegationSuccess)
    );

    assert!(
        is_collect_build_candidate(&collect_pool, &trade_no).await,
        "released collect should be eligible for BuildTx"
    );
}

#[tokio::test]
#[serial]
async fn collect_resource_result_ack_does_not_release_gate_on_failure() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let collect_pool = open_collect_pool(env).await;
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    let trade_no = format!("C_RSC_FAIL_{}", next_unique_id());
    let resource_trade_no = format!("rsc_delegate_{trade_no}");

    seed_blocked_collect(&collect_pool, &trade_no).await;
    seed_resource_result(
        &collect_pool,
        &trade_no,
        &resource_trade_no,
        ApiTradeType::Collect,
        false,
    )
    .await;

    send_resource_result_ack_via_worker(collect_pool.clone(), core_pool, &resource_trade_no)
        .await
        .expect("send resource result ack");

    let collect = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect");
    assert!(collect.resource_gate_released_at.is_none());
    assert!(collect.resource_gate_result.is_none());
}

#[tokio::test]
#[serial]
async fn withdraw_origin_resource_result_ack_does_not_release_collect_gate() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let collect_pool = open_collect_pool(env).await;
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    let trade_no = format!("C_WD_ORIGIN_SKIP_{}", next_unique_id());
    let resource_trade_no = format!("rsc_delegate_{trade_no}");

    seed_blocked_collect(&collect_pool, &trade_no).await;
    seed_resource_result(
        &collect_pool,
        "W_ORIGIN_SKIP",
        &resource_trade_no,
        ApiTradeType::Withdraw,
        true,
    )
    .await;

    send_resource_result_ack_via_worker(collect_pool.clone(), core_pool, &resource_trade_no)
        .await
        .expect("send resource result ack");

    let collect = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect");
    assert!(collect.resource_gate_released_at.is_none());
    assert!(collect.resource_gate_result.is_none());
}

#[tokio::test]
#[serial]
async fn collect_failed_resource_bypass_reopens_collect_build_flow() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let collect_pool = open_collect_pool(env).await;
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    let trade_no = format!("C_RSC_FAIL_BYPASS_{}", next_unique_id());
    let resource_trade_no = format!("rsc_delegate_{trade_no}");

    seed_blocked_collect(&collect_pool, &trade_no).await;
    seed_failed_resource_receipt_row(&collect_pool, Some(&trade_no), &resource_trade_no).await;

    assert!(
        !is_collect_build_candidate(&collect_pool, &trade_no).await,
        "blocked collect should not be eligible for BuildTx before local delegation fallback"
    );

    upload_resource_tx_exec_receipt_via_worker(collect_pool.clone(), core_pool, &resource_trade_no)
        .await
        .expect("upload resource tx exec receipt");

    let collect = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect");
    assert!(collect.resource_gate_released_at.is_none());
    assert_eq!(collect.resource_dependency_type, Some(ApiResourceDependencyType::PlatformDelegate));

    assert!(
        !is_collect_build_candidate(&collect_pool, &trade_no).await,
        "platform delegation failure should not reopen BuildTx before local delegation fallback"
    );

    let collect = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("reload collect");
    assert_eq!(collect.resource_dependency_trade_no.as_deref(), Some(resource_trade_no.as_str()));
    assert_eq!(collect.resource_dependency_type, Some(ApiResourceDependencyType::PlatformDelegate));
    assert_eq!(collect.resource_block_reason, Some(ApiResourceBlockReason::NeedPlatformDelegate));
}

#[tokio::test]
#[serial]
async fn collect_resource_tx_exec_receipt_failure_without_origin_trade_no_does_not_release_gate() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let collect_pool = open_collect_pool(env).await;
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    let trade_no = format!("C_RSC_NO_ORIGIN_{}", next_unique_id());
    let resource_trade_no = format!("rsc_delegate_{trade_no}");

    seed_blocked_collect(&collect_pool, &trade_no).await;
    seed_failed_resource_receipt_row(&collect_pool, None, &resource_trade_no).await;

    upload_resource_tx_exec_receipt_via_worker(collect_pool.clone(), core_pool, &resource_trade_no)
        .await
        .expect("upload resource tx exec receipt");

    let collect = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect");
    assert!(collect.resource_gate_released_at.is_none());
    assert!(collect.resource_gate_result.is_none());
}
