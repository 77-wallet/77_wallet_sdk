use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use chrono::Utc;
use tempfile::TempDir;
use wallet_api::infrastructure::api_trans::ShadowCollectCommand;
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool, SqliteContext,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::collect::ApiCollectRepo,
};

use crate::harness::{AssertRole, CountRole, GivenRole, LoadRole, SeedRole, ThenRole, WhenRole};

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
    query_count: RefCell<Option<Arc<AtomicUsize>>>,
    adapter_guard: RefCell<Option<TronRecoverProbeGuard>>,
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

        Self {
            _dir: dir,
            collect_pool,
            core_pool,
            query_count: RefCell::new(None),
            adapter_guard: RefCell::new(None),
        }
    }

    fn seed(&self) -> SeedRole<'_, Self> {
        SeedRole::new(self)
    }

    fn load(&self) -> LoadRole<'_, Self> {
        LoadRole::new(self)
    }

    fn count(&self) -> CountRole<'_, Self> {
        CountRole::new(self)
    }

    fn assert(&self) -> AssertRole<'_, Self> {
        AssertRole::new(self)
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait CollectRecoveryGiven {
    fn chain_probe_confirms_tx(&self, fixture: &CollectRecoveryFixture);

    fn chain_query_clears_hash_then_confirms(&self, fixture: &CollectRecoveryFixture);

    async fn expired_raw_tx_collect(&self, fixture: &CollectRecoveryFixture);

    async fn recoverable_collect_with_tx_hash(&self, fixture: &CollectRecoveryFixture);
}

#[async_trait::async_trait(?Send)]
impl CollectRecoveryGiven for GivenRole<'_, ShadowCollectRecoveryScenario> {
    fn chain_probe_confirms_tx(&self, fixture: &CollectRecoveryFixture) {
        self.scenario().seed().chain_probe_confirms_tx(fixture);
    }

    fn chain_query_clears_hash_then_confirms(&self, fixture: &CollectRecoveryFixture) {
        self.scenario().seed().chain_query_clears_hash_then_confirms(fixture);
    }

    async fn expired_raw_tx_collect(&self, fixture: &CollectRecoveryFixture) {
        self.scenario().seed().expired_raw_tx_collect(fixture).await;
    }

    async fn recoverable_collect_with_tx_hash(&self, fixture: &CollectRecoveryFixture) {
        self.scenario().seed().recoverable_collect_with_tx_hash(fixture).await;
    }
}

#[async_trait::async_trait(?Send)]
trait CollectRecoverySeed {
    fn chain_probe_confirms_tx(&self, fixture: &CollectRecoveryFixture);

    fn chain_query_clears_hash_then_confirms(&self, fixture: &CollectRecoveryFixture);

    async fn expired_raw_tx_collect(&self, fixture: &CollectRecoveryFixture);

    async fn recoverable_collect_with_tx_hash(&self, fixture: &CollectRecoveryFixture);
}

#[async_trait::async_trait(?Send)]
impl CollectRecoverySeed for SeedRole<'_, ShadowCollectRecoveryScenario> {
    fn chain_probe_confirms_tx(&self, fixture: &CollectRecoveryFixture) {
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
        self.scenario().query_count.replace(Some(query_count));
        self.scenario().adapter_guard.replace(Some(adapter_guard));
    }

    fn chain_query_clears_hash_then_confirms(&self, fixture: &CollectRecoveryFixture) {
        let clear_trade_no = fixture.trade_no.clone();
        let clear_pool = self.scenario().collect_pool.clone();
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
        self.scenario().query_count.replace(Some(query_count));
        self.scenario().adapter_guard.replace(Some(adapter_guard));
    }

    async fn expired_raw_tx_collect(&self, fixture: &CollectRecoveryFixture) {
        seed_tron_collect(&self.scenario().collect_pool, fixture).await;

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
        .execute(self.scenario().collect_pool.as_ref())
        .await
        .expect("seed expired raw tx facts");
    }

    async fn recoverable_collect_with_tx_hash(&self, fixture: &CollectRecoveryFixture) {
        seed_tron_collect(&self.scenario().collect_pool, fixture).await;

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
        .execute(self.scenario().collect_pool.as_ref())
        .await
        .expect("seed recoverable collect row");
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait CollectRecoveryWhen {
    async fn recover_runs(&self, fixture: &CollectRecoveryFixture);
}

#[async_trait::async_trait(?Send)]
impl CollectRecoveryWhen for WhenRole<'_, ShadowCollectRecoveryScenario> {
    async fn recover_runs(&self, fixture: &CollectRecoveryFixture) {
        let worker = build_shadow_collect_worker_from_pools(
            self.scenario().collect_pool.clone(),
            self.scenario().core_pool.clone(),
        );
        worker
            .handle(ShadowCollectCommand::Recover(fixture.trade_no.clone()))
            .await
            .expect("recover command should succeed");
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait CollectRecoveryThen {
    fn chain_was_queried_once(&self);

    async fn expired_raw_tx_is_confirmed_without_rebuild(&self, fixture: &CollectRecoveryFixture);

    async fn tx_hash_is_backfilled_and_receipt_upload_needed(
        &self,
        fixture: &CollectRecoveryFixture,
    );
}

#[async_trait::async_trait(?Send)]
impl CollectRecoveryThen for ThenRole<'_, ShadowCollectRecoveryScenario> {
    fn chain_was_queried_once(&self) {
        let query_count = self.scenario().count().chain_queries();
        self.scenario().assert().chain_was_queried_once(query_count);
    }

    async fn expired_raw_tx_is_confirmed_without_rebuild(&self, fixture: &CollectRecoveryFixture) {
        let after = self.scenario().load().collect(&fixture.trade_no).await;
        self.scenario().assert().expired_raw_tx_is_confirmed_without_rebuild(&after);
    }

    async fn tx_hash_is_backfilled_and_receipt_upload_needed(
        &self,
        fixture: &CollectRecoveryFixture,
    ) {
        let after = self.scenario().load().collect(&fixture.trade_no).await;
        let needs_receipt_upload =
            self.scenario().load().has_receipt_upload_candidate(&fixture.trade_no).await;
        self.scenario().assert().tx_hash_is_backfilled_and_receipt_upload_needed(
            &after,
            fixture,
            needs_receipt_upload,
        );
    }
}

#[async_trait::async_trait(?Send)]
trait CollectRecoveryLoad {
    async fn collect(&self, trade_no: &str) -> ApiCollectEntity;

    async fn has_receipt_upload_candidate(&self, trade_no: &str) -> bool;
}

#[async_trait::async_trait(?Send)]
impl CollectRecoveryLoad for LoadRole<'_, ShadowCollectRecoveryScenario> {
    async fn collect(&self, trade_no: &str) -> ApiCollectEntity {
        ApiCollectRepo::get_api_collect_by_trade_no(&self.scenario().collect_pool, trade_no)
            .await
            .expect("reload collect after recover")
    }

    async fn has_receipt_upload_candidate(&self, trade_no: &str) -> bool {
        ApiCollectRepo::scan_need_tx_exec_receipt_upload(&self.scenario().collect_pool, 10_000)
            .await
            .expect("scan need tx exec receipt upload")
            .iter()
            .any(|record| record.trade_no == trade_no)
    }
}

trait CollectRecoveryCount {
    fn chain_queries(&self) -> usize;
}

impl CollectRecoveryCount for CountRole<'_, ShadowCollectRecoveryScenario> {
    fn chain_queries(&self) -> usize {
        let query_count = self.scenario().query_count.borrow();
        query_count.as_ref().expect("probe query count installed").load(Ordering::Relaxed)
    }
}

trait CollectRecoveryAssert {
    fn chain_was_queried_once(&self, query_count: usize);

    fn expired_raw_tx_is_confirmed_without_rebuild(&self, after: &ApiCollectEntity);

    fn tx_hash_is_backfilled_and_receipt_upload_needed(
        &self,
        after: &ApiCollectEntity,
        fixture: &CollectRecoveryFixture,
        needs_receipt_upload: bool,
    );
}

impl CollectRecoveryAssert for AssertRole<'_, ShadowCollectRecoveryScenario> {
    fn chain_was_queried_once(&self, query_count: usize) {
        assert_eq!(query_count, 1, "recover must query chain first");
    }

    fn expired_raw_tx_is_confirmed_without_rebuild(&self, after: &ApiCollectEntity) {
        assert!(after.transaction_time.is_some(), "recover must persist chain confirmation");
        assert!(after.last_broadcast_at.is_some(), "broadcast evidence must be preserved");
        assert!(
            after.raw_tx.is_some(),
            "expired raw tx must not be invalidated before final confirmation"
        );
    }

    fn tx_hash_is_backfilled_and_receipt_upload_needed(
        &self,
        after: &ApiCollectEntity,
        fixture: &CollectRecoveryFixture,
        needs_receipt_upload: bool,
    ) {
        assert_eq!(after.tx_hash.as_deref(), Some(fixture.tx_hash.as_str()));
        assert!(after.transaction_time.is_some());
        assert!(
            needs_receipt_upload,
            "recovered collect with backfilled hash must enter receipt upload scan"
        );
    }
}
