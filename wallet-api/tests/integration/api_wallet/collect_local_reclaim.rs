use serial_test::serial;
use tempfile::TempDir;
use wallet_api::testkit::{
    collect::scan_collect_intent_labels_once,
    resource_reclaim::scan_local_reclaim_intent_labels_once,
};
use wallet_database::{
    ApiTransactionDbPool, SqliteContext,
    entities::{api_resource_delegation::NewApiResourceDelegation, api_trade_type::ApiTradeType},
    repositories::api_wallet::resource_delegation::ApiResourceDelegationRepo,
};

struct LocalReclaimDb {
    _dir: TempDir,
    pool: ApiTransactionDbPool,
}

impl LocalReclaimDb {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let pool =
            SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_transaction.db"))
                .await
                .expect("init api_transaction.db")
                .into_transaction_db_pool()
                .expect("transaction pool");

        Self { _dir: dir, pool }
    }

    async fn insert_local_undelegate(&self, resource_trade_no: &str, origin_trade_no: &str) {
        ApiResourceDelegationRepo::upsert(
            &self.pool,
            NewApiResourceDelegation::local_undelegate(
                "uid",
                resource_trade_no,
                origin_trade_no,
                ApiTradeType::Collect as i64,
                "owner",
                "receiver",
                "5",
                "1000",
            ),
        )
        .await
        .expect("insert local undelegate task");
    }
}

#[tokio::test]
#[serial]
async fn collect_shadow_scanner_no_longer_owns_local_undelegation_intents() {
    let db = LocalReclaimDb::new().await;
    db.insert_local_undelegate("rsc_local_undelegate_scan", "C_SCAN").await;

    let collect_labels =
        scan_collect_intent_labels_once(db.pool.clone()).await.expect("scan collect labels");
    assert!(
        collect_labels.iter().all(|label| label != "ExecuteLocalUndelegation"),
        "collect shadow should no longer own local undelegation execute"
    );
    assert!(
        collect_labels.iter().all(|label| label != "RecoverLocalUndelegation"),
        "collect shadow should no longer own local undelegation recover"
    );
}

#[tokio::test]
#[serial]
async fn local_reclaim_shadow_scanner_owns_local_undelegation_intents() {
    let db = LocalReclaimDb::new().await;
    db.insert_local_undelegate("rsc_local_undelegate_execute_scan", "C_EXECUTE_SCAN").await;
    db.insert_local_undelegate("rsc_local_undelegate_recover_scan", "C_RECOVER_SCAN").await;

    ApiResourceDelegationRepo::claim_build_slot(&db.pool, "rsc_local_undelegate_recover_scan")
        .await
        .expect("claim build slot");
    ApiResourceDelegationRepo::mark_broadcast_success(
        &db.pool,
        "rsc_local_undelegate_recover_scan",
        "tx_hash_recover_scan",
    )
    .await
    .expect("mark broadcast success");

    let reclaim_labels =
        scan_local_reclaim_intent_labels_once(db.pool).await.expect("scan reclaim labels");
    assert!(reclaim_labels.iter().any(|label| label == "ExecuteLocalUndelegation"));
    assert!(reclaim_labels.iter().any(|label| label == "RecoverLocalUndelegation"));
}
