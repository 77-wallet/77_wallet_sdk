use wallet_api::testkit::{
    collect::scan_collect_intent_labels_once,
    context::{api_trans_test_ctx, api_trans_test_pool},
};
use wallet_database::ApiTransactionDbPool;

use super::{
    db::insert_failed_delegation_ready_for_receipt_scan, fixtures::CollectResourceGateFixture,
};

pub(crate) struct LocalCollectResourceDb {
    pool: ApiTransactionDbPool,
}

impl LocalCollectResourceDb {
    pub(crate) async fn new() -> Self {
        let pool = api_trans_test_pool().await;
        Self { pool }
    }

    pub(crate) async fn given_failed_delegation_ready_for_receipt_scan(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        insert_failed_delegation_ready_for_receipt_scan(&self.pool, &fixture.resource_trade_no)
            .await;
    }

    pub(crate) async fn when_collect_scanner_runs(&self) -> Vec<String> {
        scan_collect_intent_labels_once(api_trans_test_ctx().await)
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
