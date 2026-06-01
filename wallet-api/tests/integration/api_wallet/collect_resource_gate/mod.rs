mod support;

use serial_test::serial;
use wallet_database::entities::api_resource_gate::ApiResourceGateResult;

use support::{CollectResourceGateFixture, CollectResourceGateScenario, LocalCollectResourceDb};

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

    scenario.given_blocked_collect(&fixture).await;
    scenario.given_successful_collect_resource_result(&fixture).await;

    scenario.when_resource_result_ack_is_sent(&fixture).await;

    scenario
        .then_origin_collect_gate_is_released(
            &fixture,
            ApiResourceGateResult::ResourceDelegationSuccess,
        )
        .await;
    scenario.then_collect_can_build(&fixture).await;
}

#[tokio::test]
#[serial]
async fn collect_resource_result_ack_does_not_release_gate_on_failure() {
    let scenario = CollectResourceGateScenario::new().await;
    let fixture = CollectResourceGateFixture::origin_case("C_RSC_FAIL");

    scenario.given_blocked_collect(&fixture).await;
    scenario.given_failed_collect_resource_result(&fixture).await;

    scenario.when_resource_result_ack_is_sent(&fixture).await;

    scenario.then_origin_collect_gate_is_not_released(&fixture).await;
}

#[tokio::test]
#[serial]
async fn withdraw_origin_resource_result_ack_does_not_release_collect_gate() {
    let scenario = CollectResourceGateScenario::new().await;
    let fixture = CollectResourceGateFixture::origin_case("C_WD_ORIGIN_SKIP");

    scenario.given_blocked_collect(&fixture).await;
    scenario.given_successful_withdraw_origin_resource_result(&fixture).await;

    scenario.when_resource_result_ack_is_sent(&fixture).await;

    scenario.then_origin_collect_gate_is_not_released(&fixture).await;
}

#[tokio::test]
#[serial]
async fn collect_failed_resource_bypass_reopens_collect_build_flow() {
    let scenario = CollectResourceGateScenario::new().await;
    let fixture = CollectResourceGateFixture::origin_case("C_RSC_FAIL_BYPASS");

    scenario.given_blocked_collect(&fixture).await;
    scenario.given_failed_collect_resource_receipt(&fixture).await;
    scenario.then_collect_cannot_build(&fixture).await;

    scenario.when_resource_receipt_upload_is_sent(&fixture).await;

    scenario.then_collect_still_waits_for_platform_delegate(&fixture).await;
    scenario.then_collect_cannot_build(&fixture).await;
}

#[tokio::test]
#[serial]
async fn collect_resource_tx_exec_receipt_failure_without_origin_trade_no_does_not_release_gate() {
    let scenario = CollectResourceGateScenario::new().await;
    let fixture = CollectResourceGateFixture::origin_case("C_RSC_NO_ORIGIN");

    scenario.given_blocked_collect(&fixture).await;
    scenario.given_failed_resource_receipt_without_origin_trade(&fixture).await;

    scenario.when_resource_receipt_upload_is_sent(&fixture).await;

    scenario.then_origin_collect_gate_is_not_released(&fixture).await;
}
