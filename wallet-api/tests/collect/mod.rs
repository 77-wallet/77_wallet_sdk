use chrono::Utc;
use sqlx;
use tempfile::TempDir;
use wallet_api::test::collect::build_collect_tx_exec_receipt_payload;
use wallet_database::{
    SqliteContext,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::collect::ApiCollectRepo,
};

struct TestFundsDb {
    _dir: TempDir,
    pool: wallet_database::ApiFundsDbPool,
}

impl TestFundsDb {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_funds.db"))
            .await
            .expect("init api_funds.db");
        let pool = ctx.into_collect_db_pool().expect("collect pool");
        Self { _dir: dir, pool }
    }
}

#[tokio::test]
async fn collect_blockhash_rebuild_clears_stale_build_facts_and_persists_new_to_addr() {
    let db = TestFundsDb::new().await;
    let trade_no = "T_collect_blockhash_rebuild_refresh";

    ApiCollectRepo::upsert_api_collect(
        &db.pool,
        "uid",
        "collect",
        "from",
        "old-to",
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
        "USDC",
        trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

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
    .execute(db.pool.as_ref())
    .await
    .expect("set stale build facts");

    let invalidated = ApiCollectRepo::invalidate_raw_tx_for_rebuild(&db.pool, trade_no, None)
        .await
        .expect("invalidate raw tx for rebuild");
    assert_eq!(invalidated, 1);

    let after_invalidate = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("load collect after invalidate");
    assert!(after_invalidate.raw_tx.is_none(), "stale raw_tx must be cleared");
    assert!(after_invalidate.tx_hash.is_none(), "stale tx_hash must be cleared");
    assert_eq!(
        after_invalidate.to_addr, "old-to",
        "rebuild invalidation must not invent a new execution address on its own"
    );

    ApiCollectRepo::update_api_collect_to_addr(&db.pool, trade_no, "new-to")
        .await
        .expect("persist rebuilt to_addr");

    let rebuilt = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("load rebuilt collect");
    assert!(rebuilt.raw_tx.is_none(), "rebuild starts from cleared build facts");
    assert!(rebuilt.tx_hash.is_none(), "rebuild starts from cleared tx hash");
    assert_eq!(
        rebuilt.to_addr, "new-to",
        "next build must persist the latest strategy address before generating new raw_tx"
    );
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
        token_addr: Some("token".to_string()),
        symbol: "USDC".to_string(),
        trade_no: "trade-no".to_string(),
        trade_type: 2,
        risk_addr: 1,
        status: ApiCollectStatus::SendingTx,
        nonce: 0,
        tx_hash: Some("hash".to_string()),
        transaction_fee: "0".to_string(),
        transaction_time: Some(Utc::now()),
        block_height: "0".to_string(),
        notes: String::new(),
        post_tx_count: 0,
        post_confirm_tx_count: 0,
        err_code: None,
        err_msg: String::new(),
        order_ack_attempted_at: None,
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
        result_ack_attempted_at: None,
        result_ack_sent_at: None,
        result_ack_send_count: 0,
        tx_res_received_at: None,
        service_fee_attempted_at: None,
        service_fee_uploaded_at: None,
        need_service_fee: None,
        ever_needed_service_fee: false,
        tx_fee_res_ack_sent_at: None,
        tx_exec_receipt_attempted_at: None,
        tx_exec_receipt_uploaded_at: None,
        finished_at: None,
        created_at: Utc::now(),
        updated_at: Some(Utc::now()),
    }
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
    let db = TestFundsDb::new().await;
    let trade_no = "T_collect_rebuild_then_receipt";

    ApiCollectRepo::upsert_api_collect(
        &db.pool,
        "uid",
        "collect",
        "from",
        "old-to",
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
        "USDC",
        trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

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
    .execute(db.pool.as_ref())
    .await
    .expect("set stale build facts");

    ApiCollectRepo::invalidate_raw_tx_for_rebuild(&db.pool, trade_no, None)
        .await
        .expect("invalidate raw tx for rebuild");

    sqlx::query(
        r#"
        UPDATE api_collect
        SET to_addr = $2,
            tx_hash = $3,
            raw_tx = $4,
            last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(trade_no)
    .bind("new-to")
    .bind("new-hash")
    .bind("{\"rebuilt\":true}")
    .execute(db.pool.as_ref())
    .await
    .expect("persist rebuilt execution facts");

    let rebuilt = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("load rebuilt collect");
    let payload = build_collect_tx_exec_receipt_payload(&rebuilt, trade_no);
    let payload_json = serde_json::to_value(&payload).expect("serialize payload");

    assert_eq!(payload_json["to"], "new-to");
    assert_eq!(payload_json["hash"], "new-hash");
}
