use crate::harness::{
    WorkerTestEnv, decrypt_captured_api_backend_body, ensure_worker_env, next_unique_id,
    open_api_wallet_pool, pop_request_with_retry,
};
use chrono::Utc;
use serial_test::serial;
use sqlx;
use tempfile::TempDir;
use wallet_api::test::collect::{
    build_collect_tx_exec_receipt_payload, scan_and_dispatch_collect_tx_exec_receipt_once,
    upload_collect_tx_exec_receipt_via_backend, upload_collect_tx_exec_receipt_via_worker,
};
use wallet_database::{
    ApiTransactionDbPool, SqliteContext,
    entities::{
        api_collect::{ApiCollectEntity, ApiCollectStatus},
        asset_token_key::AssetTokenKey,
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

async fn current_backend_url() -> Option<String> {
    let app_state = wallet_api::app_state::APP_STATE.read().await;
    app_state.url().backend.clone()
}

fn base_collect_for_receipt() -> ApiCollectEntity {
    ApiCollectEntity {
        id: 1,
        name: "collect".to_string(),
        uid: "uid".to_string(),
        from_addr: "from".to_string(),
        to_addr: "persisted-to".to_string(),
        value: "1.12".to_string(),
        validate: "digest".to_string(),
        chain_code: "sol".to_string(),
        token_addr: AssetTokenKey::Contract("token".to_string()),
        symbol: "USDC".to_string(),
        trade_no: "trade-no".to_string(),
        trade_type: 2,
        risk_addr: 1,
        status: ApiCollectStatus::SendingTx,
        nonce: 0,
        tx_hash: Some("hash".to_string()),
        transaction_fee: "0".to_string(),
        transaction_time: Some(Utc::now()),
        block_height: Some("0".to_string()),
        notes: Some(String::new()),
        post_tx_count: 0,
        post_confirm_tx_count: 0,
        err_code: None,
        err_msg: Some(String::new()),
        resource_check_at: None,
        resource_gate_released_at: None,
        resource_gate_result: None,
        resource_block_reason: None,
        resource_dependency_trade_no: None,
        resource_dependency_type: None,
        order_ack_sent_at: Some(Utc::now()),
        raw_tx: Some("{}".to_string()),
        resource_consume: "0".to_string(),
        building_at: None,
        last_broadcast_at: Some(Utc::now()),
        broadcast_uncertain_since_at: None,
        broadcast_uncertain_retry_count: 0,
        broadcast_uncertain_last_checked_at: None,
        broadcast_uncertain_reconciled_at: None,
        broadcast_uncertain_rebroadcast_count: 0,
        result_ack_sent_at: None,
        result_ack_send_count: 0,
        tx_res_received_at: None,
        service_fee_order_received_at: None,
        service_fee_uploaded_at: None,
        need_service_fee: None,
        ever_needed_service_fee: false,
        tx_fee_res_ack_sent_at: None,
        tx_exec_receipt_uploaded_at: None,
        finished_at: None,
        created_at: Utc::now(),
        updated_at: Some(Utc::now()),
    }
}

async fn insert_collect(
    pool: &ApiTransactionDbPool,
    trade_no: &str,
    from_addr: &str,
    to_addr: &str,
    status: ApiCollectStatus,
) {
    ApiCollectRepo::upsert_api_collect(
        pool,
        "uid",
        "collect",
        from_addr,
        to_addr,
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
        "USDC",
        trade_no,
        2,
        status,
        1,
    )
    .await
    .expect("insert collect");
}

async fn persist_stale_build_facts(pool: &ApiTransactionDbPool, trade_no: &str) {
    sqlx::query(
        r#"
        UPDATE api_collect
        SET raw_tx = $2,
            tx_hash = $3,
            status = $4,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(trade_no)
    .bind("{\"stale\":true}")
    .bind("old-hash")
    .bind(ApiCollectStatus::SendingTx)
    .execute(pool.as_ref())
    .await
    .expect("set stale build facts");
}

async fn persist_rebuilt_facts(
    pool: &ApiTransactionDbPool,
    trade_no: &str,
    to_addr: &str,
    tx_hash: &str,
) {
    sqlx::query(
        r#"
        UPDATE api_collect
        SET to_addr = $2,
            tx_hash = $3,
            raw_tx = $4,
            transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(trade_no)
    .bind(to_addr)
    .bind(tx_hash)
    .bind("{\"rebuilt\":true}")
    .execute(pool.as_ref())
    .await
    .expect("persist rebuilt execution facts");
}

#[tokio::test]
async fn collect_tx_exec_receipt_uses_persisted_to_addr() {
    let req = base_collect_for_receipt();

    let payload = build_collect_tx_exec_receipt_payload(&req, &req.trade_no);
    let payload_json = serde_json::to_value(&payload).expect("serialize payload");

    assert_eq!(payload_json["to"], "persisted-to");
    assert_eq!(payload_json["hash"], "hash");
}

#[tokio::test]
async fn collect_rebuild_then_receipt_upload_uses_rebuilt_to_addr() {
    let db = LocalCollectDb::new().await;
    let trade_no = "T_collect_rebuild_then_receipt";

    insert_collect(&db.pool, trade_no, "from", "old-to", ApiCollectStatus::Init).await;
    persist_stale_build_facts(&db.pool, trade_no).await;

    ApiCollectRepo::invalidate_raw_tx_for_rebuild(&db.pool, trade_no, None)
        .await
        .expect("invalidate raw tx for rebuild");

    persist_rebuilt_facts(&db.pool, trade_no, "new-to", "new-hash").await;

    let rebuilt = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("load rebuilt collect");
    let payload = build_collect_tx_exec_receipt_payload(&rebuilt, trade_no);
    let payload_json = serde_json::to_value(&payload).expect("serialize payload");

    assert_eq!(payload_json["to"], "new-to");
    assert_eq!(payload_json["hash"], "new-hash");
}

#[serial]
#[tokio::test]
async fn collect_side_effect_worker_marks_tx_exec_receipt_uploaded_after_rebuild() {
    let env = ensure_worker_env().await;
    env.recorder.reset();
    let backend_url = current_backend_url().await.expect("backend url set in app state");
    assert_eq!(backend_url, env.backend_url, "worker should use the mock backend URL");

    let collect_pool = open_collect_pool(env).await;
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    let trade_no = format!("T_collect_worker_receipt_{}", next_unique_id());

    insert_collect(&collect_pool, &trade_no, "from-worker", "old-to", ApiCollectStatus::Init).await;
    persist_stale_build_facts(&collect_pool, &trade_no).await;

    ApiCollectRepo::invalidate_raw_tx_for_rebuild(&collect_pool, &trade_no, None)
        .await
        .expect("invalidate raw tx for rebuild");

    persist_rebuilt_facts(&collect_pool, &trade_no, "rebuilt-to", "rebuilt-hash").await;

    upload_collect_tx_exec_receipt_via_worker(collect_pool.clone(), core_pool, &trade_no)
        .await
        .expect("upload tx exec receipt should succeed");

    let rec = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after worker upload");
    assert!(
        rec.tx_exec_receipt_uploaded_at.is_some(),
        "worker should mark uploaded_at after successful backend upload"
    );

    let payload_json = serde_json::to_value(build_collect_tx_exec_receipt_payload(&rec, &trade_no))
        .expect("serialize rebuilt payload");
    assert_eq!(payload_json["tradeNo"], trade_no);
    assert_eq!(payload_json["to"], "rebuilt-to");
    assert_eq!(payload_json["hash"], "rebuilt-hash");
    assert_eq!(payload_json["status"], "SUCCESS");
}

#[serial]
#[tokio::test]
async fn collect_backend_api_direct_upload_hits_mock_server() {
    let env = ensure_worker_env().await;
    env.recorder.reset();
    let req = ApiCollectEntity {
        trade_no: format!("T_collect_direct_backend_{}", next_unique_id()),
        tx_hash: Some("direct-hash".to_string()),
        to_addr: "direct-to".to_string(),
        from_addr: "direct-from".to_string(),
        last_broadcast_at: Some(Utc::now()),
        ..base_collect_for_receipt()
    };

    upload_collect_tx_exec_receipt_via_backend(&req, &req.trade_no)
        .await
        .expect("direct backend upload should succeed");

    let captured = pop_request_with_retry(&env.recorder)
        .await
        .expect("captured backend request for direct upload");
    assert!(
        captured.path.contains("awallet/aw/trans/executeComplete"),
        "unexpected backend path: {}",
        captured.path
    );
    let payload_json = decrypt_captured_api_backend_body(&captured.body);
    assert_eq!(payload_json["tradeNo"], req.trade_no);
    assert_eq!(payload_json["to"], "direct-to");
    assert_eq!(payload_json["hash"], "direct-hash");
    assert_eq!(payload_json["status"], "SUCCESS");
}

#[serial]
#[tokio::test]
async fn collect_scanner_dispatcher_uploads_rebuilt_tx_exec_receipt() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let collect_pool = open_collect_pool(env).await;
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    let trade_no = format!("T_collect_scan_dispatch_{}", next_unique_id());

    insert_collect(
        &collect_pool,
        &trade_no,
        "from-scan",
        "rebuilt-to",
        ApiCollectStatus::SendingTx,
    )
    .await;

    sqlx::query(
        r#"
        UPDATE api_collect
        SET tx_hash = $2,
            raw_tx = $3,
            transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            tx_exec_receipt_uploaded_at = NULL,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(&trade_no)
    .bind("scan-hash")
    .bind("{\"rebuilt\":true}")
    .execute(collect_pool.as_ref())
    .await
    .expect("persist scan facts");

    let dispatched_trade_no =
        scan_and_dispatch_collect_tx_exec_receipt_once(collect_pool.clone(), core_pool)
            .await
            .expect("scanner-dispatcher flow should succeed");
    assert_eq!(dispatched_trade_no.as_deref(), Some(trade_no.as_str()));

    let rec = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after scanner-dispatcher");
    assert!(
        rec.tx_exec_receipt_uploaded_at.is_some(),
        "scanner-dispatcher should mark uploaded_at"
    );

    let payload_json = serde_json::to_value(build_collect_tx_exec_receipt_payload(&rec, &trade_no))
        .expect("serialize scanner-dispatch payload");
    assert_eq!(payload_json["tradeNo"], trade_no);
    assert_eq!(payload_json["to"], "rebuilt-to");
    assert_eq!(payload_json["hash"], "scan-hash");
    assert_eq!(payload_json["status"], "SUCCESS");
}
