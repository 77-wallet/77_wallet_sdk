use crate::harness::{
    WorkerTestEnv, decrypt_captured_api_backend_body, ensure_worker_env, open_api_wallet_pool,
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

    pub(crate) async fn given_mock_backend_is_active(&self) {
        let backend_url = current_backend_url().await.expect("backend url set in app state");
        assert_eq!(backend_url, self.env.backend_url, "worker should use the mock backend URL");
    }

    pub(crate) async fn given_rebuilt_collect_execution(&self, fixture: &CollectReceiptFixture) {
        self.given_collect_order(fixture, ApiCollectStatus::Init).await;
        self.persist_stale_build_facts(&fixture.trade_no).await;
        self.invalidate_for_rebuild(&fixture.trade_no).await;
        self.persist_rebuilt_execution_facts(fixture).await;
    }

    pub(crate) async fn given_scanner_ready_collect_execution(
        &self,
        fixture: &CollectReceiptFixture,
    ) {
        self.given_collect_order(fixture, ApiCollectStatus::SendingTx).await;
        self.persist_rebuilt_execution_facts(fixture).await;
        self.persist_scanner_receipt_facts(fixture).await;
    }

    pub(crate) async fn when_worker_uploads_receipt(&self, fixture: &CollectReceiptFixture) {
        upload_collect_tx_exec_receipt_via_worker(
            self.collect_pool.clone(),
            self.core_pool.clone(),
            &fixture.trade_no,
        )
        .await
        .expect("upload tx exec receipt should succeed");
    }

    pub(crate) async fn when_direct_backend_uploads_receipt(
        &self,
        fixture: &CollectReceiptFixture,
    ) {
        let req = fixture.receipt_entity();

        upload_collect_tx_exec_receipt_via_backend(&req, &req.trade_no)
            .await
            .expect("direct backend upload should succeed");
    }

    pub(crate) async fn when_scanner_dispatches_receipt(&self) -> Option<String> {
        scan_and_dispatch_collect_tx_exec_receipt_once(
            self.collect_pool.clone(),
            self.core_pool.clone(),
        )
        .await
        .expect("scanner-dispatcher flow should succeed")
    }

    pub(crate) async fn then_receipt_upload_is_persisted(&self, fixture: &CollectReceiptFixture) {
        let rec = self.load_collect(&fixture.trade_no).await;
        assert_collect_tx_exec_receipt_uploaded(&rec);
    }

    pub(crate) async fn then_receipt_payload_uses_execution_facts(
        &self,
        fixture: &CollectReceiptFixture,
    ) {
        let rec = self.load_collect(&fixture.trade_no).await;
        let payload_json = collect_receipt_payload_json(&rec, &fixture.trade_no);

        assert_collect_receipt_payload(
            &payload_json,
            &fixture.trade_no,
            &fixture.receipt_to_addr,
            &fixture.tx_hash,
        );
    }

    pub(crate) async fn then_backend_received_execute_complete(
        &self,
        fixture: &CollectReceiptFixture,
    ) {
        let payload_json = self.pop_execute_complete_payload().await;

        assert_collect_receipt_payload(
            &payload_json,
            &fixture.trade_no,
            &fixture.receipt_to_addr,
            &fixture.tx_hash,
        );
    }

    pub(crate) fn then_scanner_selected_trade(
        &self,
        dispatched_trade_no: Option<String>,
        fixture: &CollectReceiptFixture,
    ) {
        assert_eq!(dispatched_trade_no.as_deref(), Some(fixture.trade_no.as_str()));
    }

    async fn given_collect_order(&self, fixture: &CollectReceiptFixture, status: ApiCollectStatus) {
        insert_collect(
            &self.collect_pool,
            &fixture.trade_no,
            &fixture.from_addr,
            &fixture.initial_to_addr,
            status,
        )
        .await;
    }

    async fn invalidate_for_rebuild(&self, trade_no: &str) {
        ApiCollectRepo::invalidate_raw_tx_for_rebuild(&self.collect_pool, trade_no, None)
            .await
            .expect("invalidate raw tx for rebuild");
    }

    async fn persist_stale_build_facts(&self, trade_no: &str) {
        persist_stale_build_facts(&self.collect_pool, trade_no).await;
    }

    async fn persist_rebuilt_execution_facts(&self, fixture: &CollectReceiptFixture) {
        persist_rebuilt_execution_facts(
            &self.collect_pool,
            &fixture.trade_no,
            &fixture.receipt_to_addr,
            &fixture.tx_hash,
        )
        .await;
    }

    async fn persist_scanner_receipt_facts(&self, fixture: &CollectReceiptFixture) {
        persist_scanner_receipt_facts(&self.collect_pool, &fixture.trade_no, &fixture.tx_hash)
            .await;
    }

    async fn load_collect(&self, trade_no: &str) -> ApiCollectEntity {
        ApiCollectRepo::get_api_collect_by_trade_no(&self.collect_pool, trade_no)
            .await
            .expect("load collect")
    }

    async fn pop_execute_complete_payload(&self) -> Value {
        let mut seen_paths = Vec::new();

        for _ in 0..20 {
            let captured = pop_request_with_retry(&self.env.recorder)
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
