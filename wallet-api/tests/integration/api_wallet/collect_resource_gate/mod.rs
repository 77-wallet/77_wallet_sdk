mod support;

use serial_test::serial;

use support::{
    CollectResourceGateFixture, CollectResourceGateGiven, CollectResourceGateScenario,
    CollectResourceGateThen, CollectResourceGateWhen, LocalCollectResourceDb, ScenarioRoles,
};

#[tokio::test]
async fn collect_scanner_emits_resource_receipt_upload_for_failed_delegation() {
    let db = LocalCollectResourceDb::new().await;
    let fixture = CollectResourceGateFixture::resource_scan_case("RSC_FAIL_RECEIPT_SCAN");

    db.given_failed_delegation_ready_for_receipt_scan(&fixture).await;

    let labels = db.when_collect_scanner_runs().await;

    db.then_scanner_emits_resource_receipt_upload(labels);
}

#[tokio::test]
#[serial]
async fn collect_resource_result_ack_releases_origin_collect_gate() {
    let scenario = CollectResourceGateScenario::new().await;
    let fixture = CollectResourceGateFixture::origin_case("C_RSC_RELEASE");
    let given = scenario.given();
    let when = scenario.when();
    let then = scenario.then();

    given.blocked_collect(&fixture).await;
    given.successful_collect_resource_result(&fixture).await;

    when.resource_result_ack_is_sent(&fixture).await;

    then.origin_collect_gate_is_released_by_successful_delegation(&fixture).await;
    then.collect_can_build(&fixture).await;
}

#[tokio::test]
#[serial]
async fn collect_resource_result_ack_does_not_release_gate_on_failure() {
    let scenario = CollectResourceGateScenario::new().await;
    let fixture = CollectResourceGateFixture::origin_case("C_RSC_FAIL");
    let given = scenario.given();
    let when = scenario.when();
    let then = scenario.then();

    given.blocked_collect(&fixture).await;
    given.failed_collect_resource_result(&fixture).await;

    when.resource_result_ack_is_sent(&fixture).await;

    then.origin_collect_gate_is_not_released(&fixture).await;
}

#[tokio::test]
#[serial]
async fn withdraw_origin_resource_result_ack_does_not_release_collect_gate() {
    let scenario = CollectResourceGateScenario::new().await;
    let fixture = CollectResourceGateFixture::origin_case("C_WD_ORIGIN_SKIP");
    let given = scenario.given();
    let when = scenario.when();
    let then = scenario.then();

    given.blocked_collect(&fixture).await;
    given.successful_withdraw_origin_resource_result(&fixture).await;

    when.resource_result_ack_is_sent(&fixture).await;

    then.origin_collect_gate_is_not_released(&fixture).await;
}

#[tokio::test]
#[serial]
async fn collect_failed_resource_bypass_reopens_collect_build_flow() {
    let scenario = CollectResourceGateScenario::new().await;
    let fixture = CollectResourceGateFixture::origin_case("C_RSC_FAIL_BYPASS");
    let given = scenario.given();
    let when = scenario.when();
    let then = scenario.then();

    given.blocked_collect(&fixture).await;
    given.failed_collect_resource_receipt(&fixture).await;
    then.collect_cannot_build(&fixture).await;

    when.resource_receipt_upload_is_sent(&fixture).await;

    then.collect_still_waits_for_platform_delegate(&fixture).await;
    then.collect_cannot_build(&fixture).await;
}

#[tokio::test]
#[serial]
async fn collect_resource_tx_exec_receipt_failure_without_origin_trade_no_does_not_release_gate() {
    let scenario = CollectResourceGateScenario::new().await;
    let fixture = CollectResourceGateFixture::origin_case("C_RSC_NO_ORIGIN");
    let given = scenario.given();
    let when = scenario.when();
    let then = scenario.then();

    given.blocked_collect(&fixture).await;
    given.failed_resource_receipt_without_origin_trade(&fixture).await;

    when.resource_receipt_upload_is_sent(&fixture).await;

    then.origin_collect_gate_is_not_released(&fixture).await;
}
