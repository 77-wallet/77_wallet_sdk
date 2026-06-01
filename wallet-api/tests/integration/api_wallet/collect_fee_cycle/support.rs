use tempfile::TempDir;
use wallet_api::testkit::collect::scan_collect_intent_labels_once;
use wallet_database::{
    ApiTransactionDbPool, SqliteContext,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::collect::ApiCollectRepo,
};

use crate::harness::next_unique_id;

pub(super) struct CollectFeeCycleFixture {
    trade_no: String,
    from_addr: &'static str,
    to_addr: &'static str,
    token_addr: Option<String>,
    symbol: &'static str,
}

impl CollectFeeCycleFixture {
    pub(super) fn stale_uploaded_fee() -> Self {
        Self {
            trade_no: format!("T_collect_scanner_stale_{}", next_unique_id()),
            from_addr: "from-scan",
            to_addr: "to-scan",
            token_addr: Some("token".to_string()),
            symbol: "USDC",
        }
    }

    pub(super) fn waiting_service_fee() -> Self {
        Self {
            trade_no: format!("T_collect_wait_fee_{}", next_unique_id()),
            from_addr: "from-wait",
            to_addr: "to-wait",
            token_addr: Some("token".to_string()),
            symbol: "USDC",
        }
    }

    pub(super) fn reopened_without_fee_upload() -> Self {
        Self {
            trade_no: format!("T_collect_reopen_build_{}", next_unique_id()),
            from_addr: "from-reopen",
            to_addr: "to-reopen",
            token_addr: Some("token".to_string()),
            symbol: "USDC",
        }
    }

    pub(super) fn completed_fee_result() -> Self {
        Self {
            trade_no: format!("T_collect_fee_ack_{}", next_unique_id()),
            from_addr: "from-sol",
            to_addr: "to-fee-ack",
            token_addr: None,
            symbol: "SOL",
        }
    }
}

pub(super) struct LocalCollectFeeCycleDb {
    _dir: TempDir,
    pool: ApiTransactionDbPool,
}

impl LocalCollectFeeCycleDb {
    pub(super) async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx =
            SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_transaction.db"))
                .await
                .expect("init api_transaction.db");
        let pool = ctx.into_transaction_db_pool().expect("transaction pool");
        Self { _dir: dir, pool }
    }

    pub(super) async fn given_stale_fee_cycle_row(&self, fixture: &CollectFeeCycleFixture) {
        self.insert_collect(fixture).await;

        sqlx::query(
            r#"
            UPDATE api_collect
            SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                need_service_fee = true,
                ever_needed_service_fee = true,
                service_fee_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                tx_fee_res_ack_sent_at = NULL,
                raw_tx = NULL,
                tx_hash = NULL,
                last_broadcast_at = NULL,
                transaction_time = NULL,
                err_code = NULL,
                finished_at = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = ?
            "#,
        )
        .bind(&fixture.trade_no)
        .execute(self.pool.as_ref())
        .await
        .expect("seed stale fee-cycle row");
    }

    pub(super) async fn given_waiting_service_fee_row(&self, fixture: &CollectFeeCycleFixture) {
        self.insert_collect(fixture).await;

        sqlx::query(
            r#"
            UPDATE api_collect
            SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                need_service_fee = true,
                ever_needed_service_fee = true,
                service_fee_uploaded_at = NULL,
                service_fee_order_received_at = NULL,
                tx_fee_res_ack_sent_at = NULL,
                resource_gate_released_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                raw_tx = NULL,
                tx_hash = NULL,
                last_broadcast_at = NULL,
                transaction_time = NULL,
                err_code = NULL,
                finished_at = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = ?
            "#,
        )
        .bind(&fixture.trade_no)
        .execute(self.pool.as_ref())
        .await
        .expect("seed waiting fee-cycle row");
    }

    pub(super) async fn given_reopened_without_service_fee_upload(
        &self,
        fixture: &CollectFeeCycleFixture,
    ) {
        self.insert_collect(fixture).await;

        sqlx::query(
            r#"
            UPDATE api_collect
            SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                need_service_fee = false,
                ever_needed_service_fee = true,
                service_fee_uploaded_at = NULL,
                service_fee_order_received_at = NULL,
                tx_fee_res_ack_sent_at = NULL,
                raw_tx = NULL,
                tx_hash = NULL,
                last_broadcast_at = NULL,
                transaction_time = NULL,
                err_code = NULL,
                finished_at = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = ?
            "#,
        )
        .bind(&fixture.trade_no)
        .execute(self.pool.as_ref())
        .await
        .expect("seed reopened fee-cycle row");
    }

    pub(super) async fn given_completed_fee_cycle_row(&self, fixture: &CollectFeeCycleFixture) {
        self.insert_collect(fixture).await;

        sqlx::query(
            r#"
            UPDATE api_collect
            SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                need_service_fee = false,
                ever_needed_service_fee = true,
                tx_fee_res_ack_sent_at = NULL,
                raw_tx = NULL,
                tx_hash = NULL,
                last_broadcast_at = NULL,
                transaction_time = NULL,
                finished_at = NULL,
                err_code = NULL,
                service_fee_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = ?
            "#,
        )
        .bind(&fixture.trade_no)
        .execute(self.pool.as_ref())
        .await
        .expect("seed completed fee-cycle row");
    }

    pub(super) async fn when_collect_scanner_runs(&self) -> Vec<String> {
        scan_collect_intent_labels_once(self.pool.clone())
            .await
            .expect("scanner round should succeed")
    }

    pub(super) async fn then_stale_fee_cycle_row_is_skipped(
        &self,
        fixture: &CollectFeeCycleFixture,
        labels: Vec<String>,
    ) {
        assert!(
            labels.is_empty(),
            "stale fee-cycle row must not re-enter build / fee-ack scanning"
        );

        let persisted = self.load_collect(fixture).await;
        assert_eq!(persisted.need_service_fee, Some(true));
        assert!(persisted.service_fee_uploaded_at.is_some());
        assert!(persisted.raw_tx.is_none());
        assert!(persisted.tx_hash.is_none());
    }

    pub(super) async fn then_upload_service_fee_is_selected_before_build(
        &self,
        fixture: &CollectFeeCycleFixture,
        labels: Vec<String>,
    ) {
        assert!(
            labels.iter().any(|label| label == "UploadServiceFee"),
            "active fee-wait row must emit UploadServiceFee immediately"
        );
        assert!(
            labels.iter().all(|label| label != "BuildTx"),
            "fee upload should not bypass fee-cycle gating into build"
        );

        let persisted_after = self.load_collect(fixture).await;
        assert_eq!(persisted_after.need_service_fee, Some(true));
        assert!(persisted_after.service_fee_order_received_at.is_none());
        assert!(persisted_after.service_fee_uploaded_at.is_none());
        assert!(persisted_after.raw_tx.is_none());
        assert!(persisted_after.tx_hash.is_none());
    }

    pub(super) async fn then_reopened_fee_cycle_continues_to_build(
        &self,
        fixture: &CollectFeeCycleFixture,
        labels: Vec<String>,
    ) {
        assert!(
            labels.iter().any(|label| label == "BuildTx"),
            "reopened row without a real fee upload must continue to BuildTx"
        );
        assert!(
            labels.iter().all(|label| label != "SendTxFeeResAck"),
            "reopened row without service_fee_uploaded_at must not ask for TxFeeResAck"
        );

        let persisted_after = self.load_collect(fixture).await;
        assert_eq!(persisted_after.need_service_fee, Some(false));
        assert!(persisted_after.service_fee_uploaded_at.is_none());
        assert!(persisted_after.tx_fee_res_ack_sent_at.is_none());
        assert!(persisted_after.raw_tx.is_none());
        assert!(persisted_after.tx_hash.is_none());
    }

    pub(super) async fn then_tx_fee_res_ack_is_selected_before_build(
        &self,
        fixture: &CollectFeeCycleFixture,
        labels: Vec<String>,
    ) {
        assert!(
            labels.iter().any(|label| label == "SendTxFeeResAck"),
            "fee-result row must emit TxFeeResAck"
        );
        assert!(
            labels.iter().all(|label| label != "BuildTx"),
            "fee-result ACK must be sent before build is allowed again"
        );

        let persisted_after = self.load_collect(fixture).await;
        assert_eq!(persisted_after.need_service_fee, Some(false));
        assert!(persisted_after.tx_fee_res_ack_sent_at.is_none());
        assert!(persisted_after.raw_tx.is_none());
    }

    async fn insert_collect(&self, fixture: &CollectFeeCycleFixture) {
        ApiCollectRepo::upsert_api_collect(
            &self.pool,
            "uid",
            "collect",
            fixture.from_addr,
            fixture.to_addr,
            "1.12",
            "digest",
            "sol",
            fixture.token_addr.clone(),
            fixture.symbol,
            &fixture.trade_no,
            2,
            ApiCollectStatus::Init,
            1,
        )
        .await
        .expect("insert collect");
    }

    async fn load_collect(&self, fixture: &CollectFeeCycleFixture) -> ApiCollectEntity {
        ApiCollectRepo::get_api_collect_by_trade_no(&self.pool, &fixture.trade_no)
            .await
            .expect("load collect after scanner round")
    }
}
