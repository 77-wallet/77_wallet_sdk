use crate::harness::{
    AssertRole, GivenRole, LoadRole, SeedRole, ThenRole, WhenRole, WorkerTestEnv,
    decrypt_captured_api_backend_body, ensure_worker_env, open_api_wallet_pool,
    pop_request_with_retry,
};
use serde_json::Value;
use wallet_api::testkit::collect::{
    scan_and_dispatch_collect_tx_exec_receipt_once, upload_collect_tx_exec_receipt_via_backend,
    upload_collect_tx_exec_receipt_via_worker,
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool, SqliteContext,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::collect::ApiCollectRepo,
};

use super::{
    db::{
        insert_collect, persist_rebuilt_execution_facts, persist_scanner_receipt_facts,
        persist_stale_build_facts,
    },
    fixtures::CollectReceiptFixture,
    payload::{
        assert_collect_receipt_payload, assert_collect_tx_exec_receipt_uploaded,
        collect_receipt_payload_json,
    },
};

pub(crate) struct CollectReceiptScenario {
    env: &'static WorkerTestEnv,
    collect_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
}

impl CollectReceiptScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let collect_pool = open_collect_pool(env).await;
        let core_pool = open_api_wallet_pool(&env.db_dir).await;

        Self { env, collect_pool, core_pool }
    }

    pub(crate) fn given(&self) -> GivenRole<'_, Self> {
        GivenRole::new(self)
    }

    pub(crate) fn when(&self) -> WhenRole<'_, Self> {
        WhenRole::new(self)
    }

    pub(crate) fn then(&self) -> ThenRole<'_, Self> {
        ThenRole::new(self)
    }

    fn seed(&self) -> SeedRole<'_, Self> {
        SeedRole::new(self)
    }

    fn load(&self) -> LoadRole<'_, Self> {
        LoadRole::new(self)
    }

    fn assert(&self) -> AssertRole<'_, Self> {
        AssertRole::new(self)
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait CollectReceiptGiven {
    async fn mock_backend_is_active(&self);

    async fn rebuilt_collect_execution(&self, fixture: &CollectReceiptFixture);

    async fn scanner_ready_collect_execution(&self, fixture: &CollectReceiptFixture);
}

#[async_trait::async_trait(?Send)]
impl CollectReceiptGiven for GivenRole<'_, CollectReceiptScenario> {
    async fn mock_backend_is_active(&self) {
        let backend_url = current_backend_url().await.expect("backend url set in app state");
        self.scenario()
            .assert()
            .mock_backend_is_active(&backend_url, &self.scenario().env.backend_url);
    }

    async fn rebuilt_collect_execution(&self, fixture: &CollectReceiptFixture) {
        self.scenario().seed().collect_order(fixture, ApiCollectStatus::Init).await;
        self.scenario().seed().stale_build_facts(&fixture.trade_no).await;
        self.scenario().seed().raw_tx_invalidated_for_rebuild(&fixture.trade_no).await;
        self.scenario().seed().rebuilt_execution_facts(fixture).await;
    }

    async fn scanner_ready_collect_execution(&self, fixture: &CollectReceiptFixture) {
        self.scenario().seed().collect_order(fixture, ApiCollectStatus::SendingTx).await;
        self.scenario().seed().rebuilt_execution_facts(fixture).await;
        self.scenario().seed().scanner_receipt_facts(fixture).await;
    }
}

#[async_trait::async_trait(?Send)]
trait CollectReceiptSeed {
    async fn collect_order(&self, fixture: &CollectReceiptFixture, status: ApiCollectStatus);

    async fn stale_build_facts(&self, trade_no: &str);

    async fn raw_tx_invalidated_for_rebuild(&self, trade_no: &str);

    async fn rebuilt_execution_facts(&self, fixture: &CollectReceiptFixture);

    async fn scanner_receipt_facts(&self, fixture: &CollectReceiptFixture);
}

#[async_trait::async_trait(?Send)]
impl CollectReceiptSeed for SeedRole<'_, CollectReceiptScenario> {
    async fn collect_order(&self, fixture: &CollectReceiptFixture, status: ApiCollectStatus) {
        insert_collect(
            &self.scenario().collect_pool,
            &fixture.trade_no,
            &fixture.from_addr,
            &fixture.initial_to_addr,
            status,
        )
        .await;
    }

    async fn stale_build_facts(&self, trade_no: &str) {
        persist_stale_build_facts(&self.scenario().collect_pool, trade_no).await;
    }

    async fn raw_tx_invalidated_for_rebuild(&self, trade_no: &str) {
        ApiCollectRepo::invalidate_raw_tx_for_rebuild(
            &self.scenario().collect_pool,
            trade_no,
            None,
        )
        .await
        .expect("invalidate raw tx for rebuild");
    }

    async fn rebuilt_execution_facts(&self, fixture: &CollectReceiptFixture) {
        persist_rebuilt_execution_facts(
            &self.scenario().collect_pool,
            &fixture.trade_no,
            &fixture.receipt_to_addr,
            &fixture.tx_hash,
        )
        .await;
    }

    async fn scanner_receipt_facts(&self, fixture: &CollectReceiptFixture) {
        persist_scanner_receipt_facts(
            &self.scenario().collect_pool,
            &fixture.trade_no,
            &fixture.tx_hash,
        )
        .await;
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait CollectReceiptWhen {
    async fn worker_uploads_receipt(&self, fixture: &CollectReceiptFixture);

    async fn direct_backend_uploads_receipt(&self, fixture: &CollectReceiptFixture);

    async fn scanner_dispatches_receipt(&self) -> Option<String>;
}

#[async_trait::async_trait(?Send)]
impl CollectReceiptWhen for WhenRole<'_, CollectReceiptScenario> {
    async fn worker_uploads_receipt(&self, fixture: &CollectReceiptFixture) {
        upload_collect_tx_exec_receipt_via_worker(
            self.scenario().collect_pool.clone(),
            self.scenario().core_pool.clone(),
            &fixture.trade_no,
        )
        .await
        .expect("upload tx exec receipt should succeed");
    }

    async fn direct_backend_uploads_receipt(&self, fixture: &CollectReceiptFixture) {
        let req = fixture.receipt_entity();

        upload_collect_tx_exec_receipt_via_backend(&req, &req.trade_no)
            .await
            .expect("direct backend upload should succeed");
    }

    async fn scanner_dispatches_receipt(&self) -> Option<String> {
        scan_and_dispatch_collect_tx_exec_receipt_once(
            self.scenario().collect_pool.clone(),
            self.scenario().core_pool.clone(),
        )
        .await
        .expect("scanner-dispatcher flow should succeed")
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait CollectReceiptThen {
    async fn receipt_upload_is_persisted(&self, fixture: &CollectReceiptFixture);

    async fn receipt_payload_uses_execution_facts(&self, fixture: &CollectReceiptFixture);

    async fn backend_received_execute_complete(&self, fixture: &CollectReceiptFixture);

    fn scanner_selected_trade(
        &self,
        dispatched_trade_no: Option<String>,
        fixture: &CollectReceiptFixture,
    );
}

#[async_trait::async_trait(?Send)]
impl CollectReceiptThen for ThenRole<'_, CollectReceiptScenario> {
    async fn receipt_upload_is_persisted(&self, fixture: &CollectReceiptFixture) {
        let rec = self.scenario().load().collect(&fixture.trade_no).await;
        self.scenario().assert().tx_exec_receipt_uploaded(&rec);
    }

    async fn receipt_payload_uses_execution_facts(&self, fixture: &CollectReceiptFixture) {
        let rec = self.scenario().load().collect(&fixture.trade_no).await;
        self.scenario().assert().receipt_payload_uses_execution_facts(&rec, fixture);
    }

    async fn backend_received_execute_complete(&self, fixture: &CollectReceiptFixture) {
        let payload_json = self.scenario().load().execute_complete_payload().await;
        self.scenario().assert().backend_received_execute_complete(&payload_json, fixture);
    }

    fn scanner_selected_trade(
        &self,
        dispatched_trade_no: Option<String>,
        fixture: &CollectReceiptFixture,
    ) {
        self.scenario().assert().scanner_selected_trade(dispatched_trade_no, fixture);
    }
}

#[async_trait::async_trait(?Send)]
trait CollectReceiptLoad {
    async fn collect(&self, trade_no: &str) -> ApiCollectEntity;

    async fn execute_complete_payload(&self) -> Value;
}

#[async_trait::async_trait(?Send)]
impl CollectReceiptLoad for LoadRole<'_, CollectReceiptScenario> {
    async fn collect(&self, trade_no: &str) -> ApiCollectEntity {
        ApiCollectRepo::get_api_collect_by_trade_no(&self.scenario().collect_pool, trade_no)
            .await
            .expect("load collect")
    }

    async fn execute_complete_payload(&self) -> Value {
        let mut seen_paths = Vec::new();

        for _ in 0..20 {
            let captured = pop_request_with_retry(&self.scenario().env.recorder)
                .await
                .expect("captured backend request for direct upload");

            if captured.path.contains("awallet/aw/trans/executeComplete") {
                return decrypt_captured_api_backend_body(&captured.body);
            }

            seen_paths.push(captured.path);
        }

        panic!("expected execute-complete backend request, saw paths: {seen_paths:?}");
    }
}

trait CollectReceiptAssert {
    fn mock_backend_is_active(&self, backend_url: &str, expected_backend_url: &str);

    fn tx_exec_receipt_uploaded(&self, collect: &ApiCollectEntity);

    fn receipt_payload_uses_execution_facts(
        &self,
        collect: &ApiCollectEntity,
        fixture: &CollectReceiptFixture,
    );

    fn backend_received_execute_complete(
        &self,
        payload_json: &Value,
        fixture: &CollectReceiptFixture,
    );

    fn scanner_selected_trade(
        &self,
        dispatched_trade_no: Option<String>,
        fixture: &CollectReceiptFixture,
    );
}

impl CollectReceiptAssert for AssertRole<'_, CollectReceiptScenario> {
    fn mock_backend_is_active(&self, backend_url: &str, expected_backend_url: &str) {
        assert_eq!(backend_url, expected_backend_url, "worker should use the mock backend URL");
    }

    fn tx_exec_receipt_uploaded(&self, collect: &ApiCollectEntity) {
        assert_collect_tx_exec_receipt_uploaded(collect);
    }

    fn receipt_payload_uses_execution_facts(
        &self,
        collect: &ApiCollectEntity,
        fixture: &CollectReceiptFixture,
    ) {
        let payload_json = collect_receipt_payload_json(collect, &fixture.trade_no);

        assert_collect_receipt_payload(
            &payload_json,
            &fixture.trade_no,
            &fixture.receipt_to_addr,
            &fixture.tx_hash,
        );
    }

    fn backend_received_execute_complete(
        &self,
        payload_json: &Value,
        fixture: &CollectReceiptFixture,
    ) {
        assert_collect_receipt_payload(
            payload_json,
            &fixture.trade_no,
            &fixture.receipt_to_addr,
            &fixture.tx_hash,
        );
    }

    fn scanner_selected_trade(
        &self,
        dispatched_trade_no: Option<String>,
        fixture: &CollectReceiptFixture,
    ) {
        assert_eq!(dispatched_trade_no.as_deref(), Some(fixture.trade_no.as_str()));
    }
}

async fn open_collect_pool(env: &WorkerTestEnv) -> ApiTransactionDbPool {
    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    collect_pool_ctx.into_transaction_db_pool().expect("transaction pool")
}

async fn current_backend_url() -> Option<String> {
    let app_state = wallet_api::app_state::APP_STATE.read().await;
    app_state.url().backend.clone()
}
