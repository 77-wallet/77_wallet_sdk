mod support;

use serial_test::serial;

use support::LocalReclaimScannerDb;

#[tokio::test]
#[serial]
async fn collect_shadow_scanner_no_longer_owns_local_undelegation_intents() {
    let db = LocalReclaimScannerDb::new().await;

    db.given_local_undelegation_task("rsc_local_undelegate_scan", "C_SCAN").await;

    let labels = db.when_collect_scanner_runs().await;

    db.then_collect_scanner_does_not_own_local_undelegation(labels);
}

#[tokio::test]
#[serial]
async fn local_reclaim_shadow_scanner_owns_local_undelegation_intents() {
    let db = LocalReclaimScannerDb::new().await;

    db.given_local_undelegation_task("rsc_local_undelegate_execute_scan", "C_EXECUTE_SCAN").await;
    db.given_broadcasted_local_undelegation_task(
        "rsc_local_undelegate_recover_scan",
        "C_RECOVER_SCAN",
        "tx_hash_recover_scan",
    )
    .await;

    let labels = db.when_local_reclaim_scanner_runs().await;

    db.then_local_reclaim_scanner_owns_execute_and_recover(labels);
}
