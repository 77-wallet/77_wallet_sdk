mod support;

use serial_test::serial;

use support::{
    CollectReceiptFixture, CollectReceiptScenario, LocalCollectDb, assert_collect_receipt_payload,
    base_collect_for_receipt, collect_receipt_payload_json,
};

#[tokio::test]
async fn collect_tx_exec_receipt_uses_persisted_to_addr() {
    // Arrange: payload input
    let req = base_collect_for_receipt();

    // Act
    let payload_json = collect_receipt_payload_json(&req, &req.trade_no);

    // Assert: persisted execution facts are used in the receipt payload.
    assert_collect_receipt_payload(&payload_json, &req.trade_no, "persisted-to", "hash");
}

#[tokio::test]
async fn collect_rebuild_then_receipt_upload_uses_rebuilt_to_addr() {
    // Arrange: local DB and stale build facts
    let db = LocalCollectDb::new().await;
    let fixture = CollectReceiptFixture::new("collect_rebuild_then_receipt");
    db.seed_stale_collect_build(&fixture).await;

    // Act: invalidate and persist rebuilt execution facts.
    db.rebuild_collect_execution(&fixture).await;

    // Assert: receipt payload uses rebuilt facts, not stale build facts.
    let payload_json = db.receipt_payload_json(&fixture).await;
    assert_collect_receipt_payload(
        &payload_json,
        &fixture.trade_no,
        &fixture.receipt_to_addr,
        &fixture.tx_hash,
    );
}

#[serial]
#[tokio::test]
async fn collect_side_effect_worker_marks_tx_exec_receipt_uploaded_after_rebuild() {
    let scenario = CollectReceiptScenario::new().await;
    let fixture = CollectReceiptFixture::new("collect_worker_receipt");

    scenario.given_mock_backend_is_active().await;
    scenario.given_rebuilt_collect_execution(&fixture).await;

    scenario.when_worker_uploads_receipt(&fixture).await;

    scenario.then_receipt_upload_is_persisted(&fixture).await;
    scenario.then_receipt_payload_uses_execution_facts(&fixture).await;
}

#[serial]
#[tokio::test]
async fn collect_backend_api_direct_upload_hits_mock_server() {
    let scenario = CollectReceiptScenario::new().await;
    let fixture = CollectReceiptFixture::new("collect_direct_backend");

    scenario.when_direct_backend_uploads_receipt(&fixture).await;

    scenario.then_backend_received_execute_complete(&fixture).await;
}

#[serial]
#[tokio::test]
async fn collect_scanner_dispatcher_uploads_rebuilt_tx_exec_receipt() {
    let scenario = CollectReceiptScenario::new().await;
    let fixture = CollectReceiptFixture::new("collect_scan_dispatch");

    scenario.given_scanner_ready_collect_execution(&fixture).await;

    let dispatched_trade_no = scenario.when_scanner_dispatches_receipt().await;

    scenario.then_scanner_selected_trade(dispatched_trade_no, &fixture);
    scenario.then_receipt_upload_is_persisted(&fixture).await;
    scenario.then_receipt_payload_uses_execution_facts(&fixture).await;
}
