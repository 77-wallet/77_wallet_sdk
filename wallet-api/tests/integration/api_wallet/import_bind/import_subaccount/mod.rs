mod support;

use serial_test::serial;

use support::{SubaccountImportFixture, SubaccountImportScenario};

#[tokio::test]
#[serial(import_bind)]
async fn import_subaccount_wallet_ok_unbound() {
    let scenario = SubaccountImportScenario::new().await;
    let import = SubaccountImportFixture::new("salt-sub", "sub-wallet");

    scenario.given_backend_accepts_unbound_subaccount();

    let uid = scenario.when_subaccount_wallet_is_imported(&import).await;

    scenario.then_subaccount_wallet_is_unbound_and_initialized(&uid).await;
    scenario.then_standard_import_backend_calls_were_sent();
}

#[tokio::test]
#[serial(import_bind)]
async fn import_subaccount_wallet_query_failure_does_not_persist_half_state() {
    let scenario = SubaccountImportScenario::new().await;
    let import = SubaccountImportFixture::new("salt-sub-fail", "sub-wallet-fail");

    scenario.given_backend_bind_info_query_fails();

    let err = scenario.when_subaccount_wallet_import_fails(&import).await;

    scenario.then_bind_info_failure_did_not_persist(err, &import).await;
    scenario.then_bind_info_failure_backend_calls_were_sent();
}

#[tokio::test]
#[serial(import_bind)]
async fn import_subaccount_wallet_sets_progress_stage_before_completion() {
    let scenario = SubaccountImportScenario::new().await;
    let import = SubaccountImportFixture::new("salt-sub-stage", "sub-wallet-stage");

    scenario.given_backend_accepts_unbound_subaccount();

    let uid = scenario.when_subaccount_wallet_is_imported(&import).await;

    scenario.then_import_returns_expected_uid_and_completes_stage(&uid, &import).await;
}

#[tokio::test]
#[serial(import_bind)]
async fn import_subaccount_wallet_uid_status_mismatch_rejected_without_persist() {
    let scenario = SubaccountImportScenario::new().await;
    let import = SubaccountImportFixture::new("salt-sub-mismatch", "sub-wallet-mismatch");

    scenario.given_backend_reports_withdrawal_uid_status();

    let err = scenario.when_subaccount_wallet_import_fails(&import).await;

    scenario.then_uid_status_mismatch_did_not_persist(err, &import).await;
    scenario.then_only_uid_check_was_called();
}
