use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use chrono::Utc;
use tempfile::TempDir;
use wallet_api::infrastructure::api_trans::ShadowCollectCommand;
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool, SqliteContext,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::collect::ApiCollectRepo,
};

use super::{
    adapters::{
        TronRecoverProbeGuard, expired_tron_raw_tx_json, install_collect_tron_recover_probe_adapter,
    },
    fixtures::CollectRecoveryFixture,
    worker::{build_shadow_collect_worker_from_pools, ensure_sol_main_coin, seed_tron_collect},
};

pub(crate) struct ShadowCollectRecoveryScenario {
    _dir: TempDir,
    collect_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
    query_count: Option<Arc<AtomicUsize>>,
    _adapter_guard: Option<TronRecoverProbeGuard>,
}

impl ShadowCollectRecoveryScenario {
    pub(crate) async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let tx_ctx =
            SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_transaction.db"))
                .await
                .expect("init api_transaction.db");
        let collect_pool = tx_ctx.into_transaction_db_pool().expect("transaction pool");
        let wallet_ctx =
            SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_wallet.db"))
                .await
                .expect("init api_wallet.db");
        let core_pool = ApiWalletDbPool::new(wallet_ctx.get_pool().expect("api wallet pool"));
        ensure_sol_main_coin(&core_pool).await;

        Self { _dir: dir, collect_pool, core_pool, query_count: None, _adapter_guard: None }
    }

    pub(crate) fn given_chain_probe_confirms_tx(&mut self, fixture: &CollectRecoveryFixture) {
        let query_count = Arc::new(AtomicUsize::new(0));
        let adapter_guard = install_collect_tron_recover_probe_adapter(
            query_count.clone(),
            None,
            &fixture.tx_hash,
            0.25,
            r#"{"net_used":0,"energy_used":0}"#,
            1_700_000_000_000,
            99,
        );
        self.query_count = Some(query_count);
        self._adapter_guard = Some(adapter_guard);
    }

    pub(crate) fn given_chain_query_clears_hash_then_confirms(
        &mut self,
        fixture: &CollectRecoveryFixture,
    ) {
        let clear_trade_no = fixture.trade_no.clone();
        let clear_pool = self.collect_pool.clone();
        let query_hook: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let pool = clear_pool.clone();
            let trade_no = clear_trade_no.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("create helper runtime");
                rt.block_on(async move {
                    let _ = sqlx::query(
                        r#"
                        UPDATE api_collect
                        SET tx_hash = ''
                        WHERE trade_no = ?
                        "#,
                    )
                    .bind(&trade_no)
                    .execute(pool.as_ref())
                    .await;
                });
            })
            .join()
            .expect("clear hash hook");
        });

        let query_count = Arc::new(AtomicUsize::new(0));
        let adapter_guard = install_collect_tron_recover_probe_adapter(
            query_count.clone(),
            Some(query_hook),
            &fixture.tx_hash,
            0.25,
            r#"{"net_used":0,"energy_used":0}"#,
            1_700_000_000_000,
            99,
        );
        self.query_count = Some(query_count);
        self._adapter_guard = Some(adapter_guard);
    }

    pub(crate) async fn given_expired_raw_tx_collect(&self, fixture: &CollectRecoveryFixture) {
        seed_tron_collect(&self.collect_pool, fixture).await;

        let expired_raw_tx = expired_tron_raw_tx_json(Utc::now().timestamp_millis() - 60_000);
        sqlx::query(
            r#"
            UPDATE api_collect
            SET raw_tx = $2,
                tx_hash = $3,
                last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                status = $4,
                transaction_time = NULL,
                tx_exec_receipt_uploaded_at = NULL,
                err_code = NULL,
                err_msg = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
            "#,
        )
        .bind(&fixture.trade_no)
        .bind(&expired_raw_tx)
        .bind(&fixture.tx_hash)
        .bind(ApiCollectStatus::SendingTx)
        .execute(self.collect_pool.as_ref())
        .await
        .expect("seed expired raw tx facts");
    }

    pub(crate) async fn given_recoverable_collect_with_tx_hash(
        &self,
        fixture: &CollectRecoveryFixture,
    ) {
        seed_tron_collect(&self.collect_pool, fixture).await;

        sqlx::query(
            r#"
            UPDATE api_collect
            SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                raw_tx = '{"tx":true}',
                tx_hash = ?,
                last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                transaction_time = NULL,
                tx_exec_receipt_uploaded_at = NULL,
                err_code = NULL,
                err_msg = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = ?
            "#,
        )
        .bind(&fixture.tx_hash)
        .bind(&fixture.trade_no)
        .execute(self.collect_pool.as_ref())
        .await
        .expect("seed recoverable collect row");
    }

    pub(crate) async fn when_recover_runs(&self, fixture: &CollectRecoveryFixture) {
        let worker = build_shadow_collect_worker_from_pools(
            self.collect_pool.clone(),
            self.core_pool.clone(),
        );
        worker
            .handle(ShadowCollectCommand::Recover(fixture.trade_no.clone()))
            .await
            .expect("recover command should succeed");
    }

    pub(crate) fn then_chain_was_queried_once(&self) {
        let query_count = self.query_count.as_ref().expect("probe query count installed");
        assert_eq!(query_count.load(Ordering::Relaxed), 1, "recover must query chain first");
    }

    pub(crate) async fn then_expired_raw_tx_is_confirmed_without_rebuild(
        &self,
        fixture: &CollectRecoveryFixture,
    ) {
        let after = self.load_collect(&fixture.trade_no).await;
        assert!(after.transaction_time.is_some(), "recover must persist chain confirmation");
        assert!(after.last_broadcast_at.is_some(), "broadcast evidence must be preserved");
        assert!(
            after.raw_tx.is_some(),
            "expired raw tx must not be invalidated before final confirmation"
        );
    }

    pub(crate) async fn then_tx_hash_is_backfilled_and_receipt_upload_needed(
        &self,
        fixture: &CollectRecoveryFixture,
    ) {
        let after = self.load_collect(&fixture.trade_no).await;
        assert_eq!(after.tx_hash.as_deref(), Some(fixture.tx_hash.as_str()));
        assert!(after.transaction_time.is_some());

        let records = ApiCollectRepo::scan_need_tx_exec_receipt_upload(&self.collect_pool, 10_000)
            .await
            .expect("scan need tx exec receipt upload");
        assert!(
            records.iter().any(|r| r.trade_no == fixture.trade_no),
            "recovered collect with backfilled hash must enter receipt upload scan"
        );
    }

    async fn load_collect(&self, trade_no: &str) -> ApiCollectEntity {
        ApiCollectRepo::get_api_collect_by_trade_no(&self.collect_pool, trade_no)
            .await
            .expect("reload collect after recover")
    }
}
