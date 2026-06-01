use tempfile::TempDir;
use wallet_api::testkit::{
    collect::scan_collect_intent_labels_once,
    resource_reclaim::scan_local_reclaim_intent_labels_once,
};
use wallet_database::{ApiTransactionDbPool, SqliteContext};

use super::db::{insert_local_undelegation, mark_local_undelegation_broadcasted};

pub(crate) struct LocalReclaimScannerDb {
    _dir: TempDir,
    pool: ApiTransactionDbPool,
}

impl LocalReclaimScannerDb {
    pub(crate) async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let pool =
            SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_transaction.db"))
                .await
                .expect("init api_transaction.db")
                .into_transaction_db_pool()
                .expect("transaction pool");

        Self { _dir: dir, pool }
    }

    pub(crate) async fn given_local_undelegation_task(
        &self,
        resource_trade_no: &str,
        origin_trade_no: &str,
    ) {
        insert_local_undelegation(&self.pool, resource_trade_no, origin_trade_no).await;
    }

    pub(crate) async fn given_broadcasted_local_undelegation_task(
        &self,
        resource_trade_no: &str,
        origin_trade_no: &str,
        tx_hash: &str,
    ) {
        insert_local_undelegation(&self.pool, resource_trade_no, origin_trade_no).await;
        mark_local_undelegation_broadcasted(&self.pool, resource_trade_no, tx_hash).await;
    }

    pub(crate) async fn when_collect_scanner_runs(&self) -> Vec<String> {
        scan_collect_intent_labels_once(self.pool.clone()).await.expect("scan collect labels")
    }

    pub(crate) async fn when_local_reclaim_scanner_runs(&self) -> Vec<String> {
        scan_local_reclaim_intent_labels_once(self.pool.clone()).await.expect("scan reclaim labels")
    }

    pub(crate) fn then_collect_scanner_does_not_own_local_undelegation(&self, labels: Vec<String>) {
        assert!(
            labels.iter().all(|label| label != "ExecuteLocalUndelegation"),
            "collect shadow should no longer own local undelegation execute"
        );
        assert!(
            labels.iter().all(|label| label != "RecoverLocalUndelegation"),
            "collect shadow should no longer own local undelegation recover"
        );
    }

    pub(crate) fn then_local_reclaim_scanner_owns_execute_and_recover(&self, labels: Vec<String>) {
        assert!(labels.iter().any(|label| label == "ExecuteLocalUndelegation"));
        assert!(labels.iter().any(|label| label == "RecoverLocalUndelegation"));
    }
}
