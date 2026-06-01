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

pub(super) struct LocalReclaimScannerDb {
    _dir: TempDir,
    pool: ApiTransactionDbPool,
}

impl LocalReclaimScannerDb {
    pub(super) async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let pool =
            SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_transaction.db"))
                .await
                .expect("init api_transaction.db")
                .into_transaction_db_pool()
                .expect("transaction pool");

        Self { _dir: dir, pool }
    }

    pub(super) async fn given_local_undelegation_task(
        &self,
        resource_trade_no: &str,
        origin_trade_no: &str,
    ) {
        self.insert_local_undelegation(resource_trade_no, origin_trade_no).await;
    }

    pub(super) async fn given_broadcasted_local_undelegation_task(
        &self,
        resource_trade_no: &str,
        origin_trade_no: &str,
        tx_hash: &str,
    ) {
        self.insert_local_undelegation(resource_trade_no, origin_trade_no).await;

        ApiResourceDelegationRepo::claim_build_slot(&self.pool, resource_trade_no)
            .await
            .expect("claim build slot");
        ApiResourceDelegationRepo::mark_broadcast_success(&self.pool, resource_trade_no, tx_hash)
            .await
            .expect("mark broadcast success");
    }

    pub(super) async fn when_collect_scanner_runs(&self) -> Vec<String> {
        scan_collect_intent_labels_once(self.pool.clone()).await.expect("scan collect labels")
    }

    pub(super) async fn when_local_reclaim_scanner_runs(&self) -> Vec<String> {
        scan_local_reclaim_intent_labels_once(self.pool.clone()).await.expect("scan reclaim labels")
    }

    pub(super) fn then_collect_scanner_does_not_own_local_undelegation(&self, labels: Vec<String>) {
        assert!(
            labels.iter().all(|label| label != "ExecuteLocalUndelegation"),
            "collect shadow should no longer own local undelegation execute"
        );
        assert!(
            labels.iter().all(|label| label != "RecoverLocalUndelegation"),
            "collect shadow should no longer own local undelegation recover"
        );
    }

    pub(super) fn then_local_reclaim_scanner_owns_execute_and_recover(&self, labels: Vec<String>) {
        assert!(labels.iter().any(|label| label == "ExecuteLocalUndelegation"));
        assert!(labels.iter().any(|label| label == "RecoverLocalUndelegation"));
    }

    async fn insert_local_undelegation(&self, resource_trade_no: &str, origin_trade_no: &str) {
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
