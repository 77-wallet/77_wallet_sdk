use crate::harness::next_unique_id;
use sqlx;
use tempfile::TempDir;
use wallet_api::testkit::collect::scan_collect_intent_labels_once;
use wallet_database::{
    ApiTransactionDbPool, SqliteContext, entities::api_collect::ApiCollectStatus,
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

async fn insert_collect(
    pool: &ApiTransactionDbPool,
    trade_no: &str,
    from_addr: &str,
    to_addr: &str,
    token_addr: Option<String>,
    symbol: &str,
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
        token_addr,
        symbol,
        trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");
}

#[tokio::test]
async fn collect_scanner_skips_stale_fee_cycle_rows() {
    let db = LocalCollectDb::new().await;
    let trade_no = format!("T_collect_scanner_stale_{}", next_unique_id());

    insert_collect(&db.pool, &trade_no, "from-scan", "to-scan", Some("token".to_string()), "USDC")
        .await;

    sqlx::query(
        r#"
        UPDATE api_collect
        SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            need_service_fee = true,
            ever_needed_service_fee = true,
            service_fee_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            tx_fee_res_ack_sent_at = NULL,
            raw_tx = NULL,
            tx_hash = NULL,
            last_broadcast_at = NULL,
            transaction_time = NULL,
            err_code = NULL,
            finished_at = NULL,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = ?
        "#,
    )
    .bind(&trade_no)
    .execute(db.pool.as_ref())
    .await
    .expect("seed stale fee-cycle row");

    let labels = scan_collect_intent_labels_once(db.pool.clone())
        .await
        .expect("scanner round should succeed");

    assert!(labels.is_empty(), "stale fee-cycle row must not re-enter build / fee-ack scanning");

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, &trade_no)
        .await
        .expect("load collect after scanner round");
    assert_eq!(persisted.need_service_fee, Some(true));
    assert!(persisted.service_fee_uploaded_at.is_some());
    assert!(persisted.raw_tx.is_none());
    assert!(persisted.tx_hash.is_none());
}

#[tokio::test]
async fn collect_scanner_emits_upload_service_fee_when_need_service_fee_is_true() {
    let db = LocalCollectDb::new().await;
    let trade_no = format!("T_collect_wait_fee_{}", next_unique_id());

    insert_collect(&db.pool, &trade_no, "from-wait", "to-wait", Some("token".to_string()), "USDC")
        .await;

    sqlx::query(
        r#"
        UPDATE api_collect
        SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            need_service_fee = true,
            ever_needed_service_fee = true,
            service_fee_uploaded_at = NULL,
            service_fee_order_received_at = NULL,
            tx_fee_res_ack_sent_at = NULL,
            resource_gate_released_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            raw_tx = NULL,
            tx_hash = NULL,
            last_broadcast_at = NULL,
            transaction_time = NULL,
            err_code = NULL,
            finished_at = NULL,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = ?
        "#,
    )
    .bind(&trade_no)
    .execute(db.pool.as_ref())
    .await
    .expect("seed waiting fee-cycle row");

    let labels = scan_collect_intent_labels_once(db.pool.clone())
        .await
        .expect("scanner round should succeed");

    assert!(
        labels.iter().any(|label| label == "UploadServiceFee"),
        "active fee-wait row must emit UploadServiceFee immediately"
    );
    assert!(
        labels.iter().all(|label| label != "BuildTx"),
        "fee upload should not bypass fee-cycle gating into build"
    );

    let persisted_after = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, &trade_no)
        .await
        .expect("load collect after scanner round");
    assert_eq!(persisted_after.need_service_fee, Some(true));
    assert!(persisted_after.service_fee_order_received_at.is_none());
    assert!(persisted_after.service_fee_uploaded_at.is_none());
    assert!(persisted_after.raw_tx.is_none());
    assert!(persisted_after.tx_hash.is_none());
}

#[tokio::test]
async fn collect_scanner_builds_after_fee_cycle_reopen_without_service_fee_upload() {
    let db = LocalCollectDb::new().await;
    let trade_no = format!("T_collect_reopen_build_{}", next_unique_id());

    insert_collect(
        &db.pool,
        &trade_no,
        "from-reopen",
        "to-reopen",
        Some("token".to_string()),
        "USDC",
    )
    .await;

    sqlx::query(
        r#"
        UPDATE api_collect
        SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            need_service_fee = false,
            ever_needed_service_fee = true,
            -- Simulate a reopened fee cycle without a real service-fee upload
            -- in the current cycle. Historical ACK residue must not block BuildTx.
            service_fee_uploaded_at = NULL,
            service_fee_order_received_at = NULL,
            tx_fee_res_ack_sent_at = NULL,
            raw_tx = NULL,
            tx_hash = NULL,
            last_broadcast_at = NULL,
            transaction_time = NULL,
            err_code = NULL,
            finished_at = NULL,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = ?
        "#,
    )
    .bind(&trade_no)
    .execute(db.pool.as_ref())
    .await
    .expect("seed reopened fee-cycle row");

    let labels = scan_collect_intent_labels_once(db.pool.clone())
        .await
        .expect("scanner round should succeed");

    assert!(
        labels.iter().any(|label| label == "BuildTx"),
        "reopened row without a real fee upload must continue to BuildTx"
    );
    assert!(
        labels.iter().all(|label| label != "SendTxFeeResAck"),
        "reopened row without service_fee_uploaded_at must not ask for TxFeeResAck"
    );

    let persisted_after = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, &trade_no)
        .await
        .expect("load collect after scanner round");
    assert_eq!(persisted_after.need_service_fee, Some(false));
    assert!(persisted_after.service_fee_uploaded_at.is_none());
    assert!(persisted_after.tx_fee_res_ack_sent_at.is_none());
    assert!(persisted_after.raw_tx.is_none());
    assert!(persisted_after.tx_hash.is_none());
}

#[tokio::test]
async fn collect_scanner_emits_tx_fee_res_ack_before_build_after_fee_result() {
    let db = LocalCollectDb::new().await;
    let trade_no = format!("T_collect_fee_ack_{}", next_unique_id());

    insert_collect(&db.pool, &trade_no, "from-sol", "to-fee-ack", None, "SOL").await;

    sqlx::query(
        r#"
        UPDATE api_collect
        SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            need_service_fee = false,
            ever_needed_service_fee = true,
            tx_fee_res_ack_sent_at = NULL,
            raw_tx = NULL,
            tx_hash = NULL,
            last_broadcast_at = NULL,
            transaction_time = NULL,
            finished_at = NULL,
            err_code = NULL,
            service_fee_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = ?
        "#,
    )
    .bind(&trade_no)
    .execute(db.pool.as_ref())
    .await
    .expect("seed completed fee-cycle row");

    let labels = scan_collect_intent_labels_once(db.pool.clone())
        .await
        .expect("scanner round should succeed");

    assert!(
        labels.iter().any(|label| label == "SendTxFeeResAck"),
        "fee-result row must emit TxFeeResAck"
    );
    assert!(
        labels.iter().all(|label| label != "BuildTx"),
        "fee-result ACK must be sent before build is allowed again"
    );

    let persisted_after = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, &trade_no)
        .await
        .expect("load collect after scanner round");
    assert_eq!(persisted_after.need_service_fee, Some(false));
    assert!(persisted_after.tx_fee_res_ack_sent_at.is_none());
    assert!(persisted_after.raw_tx.is_none());
}
