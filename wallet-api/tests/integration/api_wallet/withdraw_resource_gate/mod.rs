mod support;

use serial_test::serial;
use wallet_database::entities::api_resource_gate::ApiResourceGateResult;

use support::{WithdrawResourceGateFixture, WithdrawResourceGateScenario};

#[serial]
#[tokio::test]
async fn withdraw_resource_result_ack_uses_wd_rsc_dl_type() {
    let scenario = WithdrawResourceGateScenario::new().await;
    let fixture = WithdrawResourceGateFixture::ack_payload_case("RSC_WD_ACK");

    scenario.given_resource_delegation_ready_for_ack(&fixture).await;

    scenario.when_resource_result_ack_is_sent(&fixture).await;

    scenario.then_resource_result_ack_uses_withdraw_resource_type(&fixture).await;
}

#[serial]
#[tokio::test]
async fn withdraw_resource_result_ack_releases_origin_withdraw_gate() {
    let scenario = WithdrawResourceGateScenario::new().await;
    let fixture = WithdrawResourceGateFixture::origin_case("W_RSC_RELEASE");

    scenario.given_blocked_withdraw(&fixture).await;
    scenario.given_successful_withdraw_resource_delegation(&fixture).await;

    scenario.when_resource_result_ack_is_sent(&fixture).await;

    scenario
        .then_origin_withdraw_gate_is_released(
            &fixture,
            ApiResourceGateResult::ResourceDelegationSuccess,
        )
        .await;
    scenario.then_withdraw_can_build(&fixture).await;
}

#[serial]
#[tokio::test]
async fn withdraw_failed_resource_bypass_reopens_withdraw_build_flow() {
    let scenario = WithdrawResourceGateScenario::new().await;
    let fixture = WithdrawResourceGateFixture::origin_case("W_RSC_FAIL");

    scenario.given_blocked_withdraw(&fixture).await;
    scenario.given_failed_withdraw_resource_delegation(&fixture).await;
    scenario.then_withdraw_cannot_build(&fixture).await;

    scenario.when_resource_receipt_upload_is_sent(&fixture).await;

    scenario
        .then_origin_withdraw_gate_is_released(
            &fixture,
            ApiResourceGateResult::ResourceDelegationFailedBypass,
        )
        .await;
    scenario.then_withdraw_can_build(&fixture).await;
}

#[serial]
#[tokio::test]
async fn withdraw_resource_result_ack_without_origin_trade_no_does_not_release_gate() {
    let scenario = WithdrawResourceGateScenario::new().await;
    let fixture = WithdrawResourceGateFixture::origin_case("W_RSC_NO_ORIGIN");

    scenario.given_blocked_withdraw(&fixture).await;
    scenario.given_resource_delegation_without_origin_trade(&fixture).await;

    scenario.when_resource_result_ack_is_sent(&fixture).await;

    scenario.then_origin_withdraw_gate_is_not_released(&fixture).await;
}

#[serial]
#[tokio::test]
async fn withdraw_resource_result_ack_for_collect_origin_does_not_release_withdraw_gate() {
    let scenario = WithdrawResourceGateScenario::new().await;
    let fixture = WithdrawResourceGateFixture::origin_case("W_RSC_WRONG_ORIGIN");

    scenario.given_blocked_withdraw(&fixture).await;
    scenario.given_collect_origin_resource_delegation(&fixture).await;

    scenario.when_resource_result_ack_is_sent(&fixture).await;

    scenario.then_origin_withdraw_gate_is_not_released(&fixture).await;
}
