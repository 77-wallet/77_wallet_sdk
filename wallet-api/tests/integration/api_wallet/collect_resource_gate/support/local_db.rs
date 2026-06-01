use tempfile::TempDir;
use wallet_api::testkit::collect::scan_collect_intent_labels_once;
use wallet_database::{ApiTransactionDbPool, SqliteContext};

use super::{
    db::insert_failed_delegation_ready_for_receipt_scan, fixtures::CollectResourceGateFixture,
};

pub(crate) struct LocalCollectResourceDb {
    _dir: TempDir,
    pool: ApiTransactionDbPool,
}

impl LocalCollectResourceDb {
    pub(crate) async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx =
            SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_transaction.db"))
                .await
                .expect("init api_transaction.db");
        let pool = ctx.into_transaction_db_pool().expect("transaction pool");
        Self { _dir: dir, pool }
    }

    pub(crate) async fn given_failed_delegation_ready_for_receipt_scan(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        insert_failed_delegation_ready_for_receipt_scan(&self.pool, &fixture.resource_trade_no)
            .await;
    }

    pub(crate) async fn when_collect_scanner_runs(&self) -> Vec<String> {
        scan_collect_intent_labels_once(self.pool.clone())
            .await
            .expect("scanner round should succeed")
    }

    pub(crate) fn then_scanner_emits_resource_receipt_upload(&self, labels: Vec<String>) {
        assert!(
            labels.iter().any(|label| label == "UploadResourceTxExecReceipt"),
            "failed resource delegation should emit UploadResourceTxExecReceipt"
        );
    }
}
