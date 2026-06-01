mod support;

use serial_test::serial;

use support::BindRelationScenario;

#[tokio::test]
#[serial(import_bind)]
async fn scan_bind_ok_calls_backend_and_persists_bind_sn_and_relation() {
    let scenario = BindRelationScenario::new().await;
    let pair = scenario.given_wallet_pair().await;

    scenario.when_scan_bind_succeeds(&pair, "scan-app-id", "scan-merchant-id").await;

    scenario.then_pair_has_bind_fields(&pair, "scan-app-id", "scan-merchant-id").await;
    scenario.then_scan_bind_backend_called_once(&pair, "scan-app-id");
}

#[tokio::test]
#[serial(import_bind)]
async fn import_bind_ok_calls_appid_import_and_persists_bind_sn_and_relation() {
    let scenario = BindRelationScenario::new().await;
    let pair = scenario.given_wallet_pair().await;

    scenario.when_import_bind_succeeds(&pair, "import-bind-merchant", "import-bind-app").await;

    scenario.then_pair_has_bind_fields(&pair, "import-bind-app", "import-bind-merchant").await;
    scenario.then_appid_import_backend_called_once(&pair);
}

#[tokio::test]
#[serial(import_bind)]
async fn import_bind_backend_fail_does_not_persist_relation() {
    let scenario = BindRelationScenario::new().await;

    scenario.given_import_bind_backend_fails("import bind backend fail");
    let pair = scenario.given_wallet_pair().await;
    let before = scenario.given_pair_bind_snapshot(&pair).await;

    let err =
        scenario.when_import_bind_fails(&pair, "import-fail-merchant", "import-fail-app").await;

    scenario.then_error_contains(err, "import bind backend fail");
    scenario.then_pair_bind_snapshot_is_unchanged(&pair, before).await;
    scenario.then_appid_import_backend_attempted_once();
}

#[tokio::test]
#[serial(import_bind)]
async fn scan_bind_backend_fail_does_not_persist_bind() {
    let scenario = BindRelationScenario::new().await;

    scenario.given_scan_bind_backend_fails("scan bind backend fail");
    let pair = scenario.given_wallet_pair().await;
    let before = scenario.given_pair_bind_snapshot(&pair).await;

    let err = scenario.when_scan_bind_fails(&pair, "scan-fail-app", "scan-fail-merchant").await;

    scenario.then_error_contains(err, "scan bind backend fail");
    scenario.then_pair_bind_snapshot_is_unchanged(&pair, before).await;
    scenario.then_scan_bind_backend_attempted_once();
}

#[tokio::test]
#[serial(import_bind)]
async fn import_bind_missing_wallet_returns_not_found_and_no_backend_call() {
    let scenario = BindRelationScenario::new().await;

    let recharge_uid = scenario.given_only_recharge_wallet("import-bind-only-recharge").await;
    let before = scenario.given_wallet_bind_snapshot(&recharge_uid).await;

    let err = scenario.when_import_bind_missing_withdrawal_fails(&recharge_uid).await;

    scenario
        .then_missing_wallet_rejection_keeps_recharge_unchanged(err, &recharge_uid, before)
        .await;
    scenario.then_appid_import_backend_was_not_called();
}

#[tokio::test]
#[serial(import_bind)]
async fn scan_bind_remote_first_then_persist() {
    let scenario = BindRelationScenario::new().await;

    scenario.given_scan_bind_backend_fails("remote bind failed first");
    let pair = scenario.given_wallet_pair().await;
    let before = scenario.given_pair_bind_snapshot(&pair).await;

    let err =
        scenario.when_scan_bind_fails(&pair, "orchestration-app", "orchestration-merchant").await;

    scenario.then_error_contains(err, "remote bind failed first");
    scenario.then_pair_bind_snapshot_is_unchanged(&pair, before).await;
}
