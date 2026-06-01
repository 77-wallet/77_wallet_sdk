use crate::harness::{
    WorkerTestEnv, decrypt_captured_api_backend_body, ensure_worker_env, next_unique_id,
    open_api_wallet_pool, pop_request_with_retry,
};
use chrono::Utc;
use serde_json::Value;
use sqlx;
use tempfile::TempDir;
use wallet_api::testkit::collect::{
    build_collect_tx_exec_receipt_payload, scan_and_dispatch_collect_tx_exec_receipt_once,
    upload_collect_tx_exec_receipt_via_backend, upload_collect_tx_exec_receipt_via_worker,
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool, SqliteContext,
    entities::{
        api_collect::{ApiCollectEntity, ApiCollectStatus},
        asset_token_key::AssetTokenKey,
    },
    repositories::api_wallet::collect::ApiCollectRepo,
};

pub(super) struct CollectReceiptFixture {
    pub(super) trade_no: String,
    from_addr: String,
    initial_to_addr: String,
    pub(super) receipt_to_addr: String,
    pub(super) tx_hash: String,
}

impl CollectReceiptFixture {
    pub(super) fn new(prefix: &str) -> Self {
        let id = next_unique_id();
        Self {
            trade_no: format!("T_{prefix}_{id}"),
            from_addr: format!("from-{prefix}-{id}"),
            initial_to_addr: format!("old-to-{prefix}-{id}"),
            receipt_to_addr: format!("receipt-to-{prefix}-{id}"),
            tx_hash: format!("hash-{prefix}-{id}"),
        }
    }

    fn receipt_entity(&self) -> ApiCollectEntity {
        ApiCollectEntity {
            trade_no: self.trade_no.clone(),
            tx_hash: Some(self.tx_hash.clone()),
            to_addr: self.receipt_to_addr.clone(),
            from_addr: self.from_addr.clone(),
            last_broadcast_at: Some(Utc::now()),
            ..base_collect_for_receipt()
        }
    }
}

pub(super) struct LocalCollectDb {
    _dir: TempDir,
    pool: ApiTransactionDbPool,
}

impl LocalCollectDb {
    pub(super) async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx =
            SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_transaction.db"))
                .await
                .expect("init api_transaction.db");
        let pool = ctx.into_transaction_db_pool().expect("transaction pool");
        Self { _dir: dir, pool }
    }

    pub(super) async fn seed_stale_collect_build(&self, fixture: &CollectReceiptFixture) {
        insert_collect(
            &self.pool,
            &fixture.trade_no,
            &fixture.from_addr,
            &fixture.initial_to_addr,
            ApiCollectStatus::Init,
        )
        .await;
        persist_stale_build_facts(&self.pool, &fixture.trade_no).await;
    }

    pub(super) async fn rebuild_collect_execution(&self, fixture: &CollectReceiptFixture) {
        ApiCollectRepo::invalidate_raw_tx_for_rebuild(&self.pool, &fixture.trade_no, None)
            .await
            .expect("invalidate raw tx for rebuild");
        persist_rebuilt_execution_facts(
            &self.pool,
            &fixture.trade_no,
            &fixture.receipt_to_addr,
            &fixture.tx_hash,
        )
        .await;
    }

    pub(super) async fn receipt_payload_json(&self, fixture: &CollectReceiptFixture) -> Value {
        let rebuilt = self.load_collect(&fixture.trade_no).await;
        collect_receipt_payload_json(&rebuilt, &fixture.trade_no)
    }

    async fn load_collect(&self, trade_no: &str) -> ApiCollectEntity {
        ApiCollectRepo::get_api_collect_by_trade_no(&self.pool, trade_no)
            .await
            .expect("load collect")
    }
}

pub(super) struct CollectReceiptScenario {
    env: &'static WorkerTestEnv,
    collect_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
}

impl CollectReceiptScenario {
    pub(super) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let collect_pool = open_collect_pool(env).await;
        let core_pool = open_api_wallet_pool(&env.db_dir).await;

        Self { env, collect_pool, core_pool }
    }

    pub(super) async fn given_mock_backend_is_active(&self) {
        let backend_url = current_backend_url().await.expect("backend url set in app state");
        assert_eq!(backend_url, self.env.backend_url, "worker should use the mock backend URL");
    }

    pub(super) async fn given_rebuilt_collect_execution(&self, fixture: &CollectReceiptFixture) {
        self.given_collect_order(fixture, ApiCollectStatus::Init).await;
        self.persist_stale_build_facts(&fixture.trade_no).await;
        self.invalidate_for_rebuild(&fixture.trade_no).await;
        self.persist_rebuilt_execution_facts(fixture).await;
    }

    pub(super) async fn given_scanner_ready_collect_execution(
        &self,
        fixture: &CollectReceiptFixture,
    ) {
        self.given_collect_order(fixture, ApiCollectStatus::SendingTx).await;
        self.persist_rebuilt_execution_facts(fixture).await;
        self.persist_scanner_receipt_facts(fixture).await;
    }

    pub(super) async fn when_worker_uploads_receipt(&self, fixture: &CollectReceiptFixture) {
        upload_collect_tx_exec_receipt_via_worker(
            self.collect_pool.clone(),
            self.core_pool.clone(),
            &fixture.trade_no,
        )
        .await
        .expect("upload tx exec receipt should succeed");
    }

    pub(super) async fn when_direct_backend_uploads_receipt(
        &self,
        fixture: &CollectReceiptFixture,
    ) {
        let req = fixture.receipt_entity();

        upload_collect_tx_exec_receipt_via_backend(&req, &req.trade_no)
            .await
            .expect("direct backend upload should succeed");
    }

    pub(super) async fn when_scanner_dispatches_receipt(&self) -> Option<String> {
        scan_and_dispatch_collect_tx_exec_receipt_once(
            self.collect_pool.clone(),
            self.core_pool.clone(),
        )
        .await
        .expect("scanner-dispatcher flow should succeed")
    }

    pub(super) async fn then_receipt_upload_is_persisted(&self, fixture: &CollectReceiptFixture) {
        let rec = self.load_collect(&fixture.trade_no).await;
        assert_collect_tx_exec_receipt_uploaded(&rec);
    }

    pub(super) async fn then_receipt_payload_uses_execution_facts(
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

    pub(super) async fn then_backend_received_execute_complete(
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

    pub(super) fn then_scanner_selected_trade(
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

pub(super) fn base_collect_for_receipt() -> ApiCollectEntity {
    ApiCollectEntity {
        id: 1,
        name: "collect".to_string(),
        uid: "uid".to_string(),
        from_addr: "from".to_string(),
        to_addr: "persisted-to".to_string(),
        value: "1.12".to_string(),
        validate: "digest".to_string(),
        chain_code: "sol".to_string(),
        token_addr: AssetTokenKey::Contract("token".to_string()),
        symbol: "USDC".to_string(),
        trade_no: "trade-no".to_string(),
        trade_type: 2,
        risk_addr: 1,
        status: ApiCollectStatus::SendingTx,
        nonce: 0,
        tx_hash: Some("hash".to_string()),
        transaction_fee: "0".to_string(),
        transaction_time: Some(Utc::now()),
        block_height: Some("0".to_string()),
        notes: Some(String::new()),
        post_tx_count: 0,
        post_confirm_tx_count: 0,
        err_code: None,
        err_msg: Some(String::new()),
        resource_check_at: None,
        resource_gate_released_at: None,
        resource_gate_result: None,
        resource_block_reason: None,
        resource_dependency_trade_no: None,
        resource_dependency_type: None,
        order_ack_sent_at: Some(Utc::now()),
        raw_tx: Some("{}".to_string()),
        resource_consume: "0".to_string(),
        building_at: None,
        last_broadcast_at: Some(Utc::now()),
        broadcast_uncertain_since_at: None,
        broadcast_uncertain_retry_count: 0,
        broadcast_uncertain_last_checked_at: None,
        broadcast_uncertain_reconciled_at: None,
        broadcast_uncertain_rebroadcast_count: 0,
        result_ack_sent_at: None,
        result_ack_send_count: 0,
        tx_res_received_at: None,
        service_fee_order_received_at: None,
        service_fee_uploaded_at: None,
        need_service_fee: None,
        ever_needed_service_fee: false,
        tx_fee_res_ack_sent_at: None,
        tx_exec_receipt_uploaded_at: None,
        finished_at: None,
        created_at: Utc::now(),
        updated_at: Some(Utc::now()),
    }
}

async fn insert_collect(
    pool: &ApiTransactionDbPool,
    trade_no: &str,
    from_addr: &str,
    to_addr: &str,
    status: ApiCollectStatus,
) {
    ApiCollectRepo::upsert_api_collect(
        pool,
        "uid",
        "collect",
        from_addr,
        to_addr,
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
        "USDC",
        trade_no,
        2,
        status,
        1,
    )
    .await
    .expect("insert collect");
}

async fn persist_stale_build_facts(pool: &ApiTransactionDbPool, trade_no: &str) {
    sqlx::query(
        r#"
        UPDATE api_collect
        SET raw_tx = $2,
            tx_hash = $3,
            status = $4,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(trade_no)
    .bind("{\"stale\":true}")
    .bind("old-hash")
    .bind(ApiCollectStatus::SendingTx)
    .execute(pool.as_ref())
    .await
    .expect("set stale build facts");
}

async fn persist_rebuilt_execution_facts(
    pool: &ApiTransactionDbPool,
    trade_no: &str,
    to_addr: &str,
    tx_hash: &str,
) {
    sqlx::query(
        r#"
        UPDATE api_collect
        SET to_addr = $2,
            tx_hash = $3,
            raw_tx = $4,
            transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(trade_no)
    .bind(to_addr)
    .bind(tx_hash)
    .bind("{\"rebuilt\":true}")
    .execute(pool.as_ref())
    .await
    .expect("persist rebuilt execution facts");
}

async fn persist_scanner_receipt_facts(pool: &ApiTransactionDbPool, trade_no: &str, tx_hash: &str) {
    sqlx::query(
        r#"
        UPDATE api_collect
        SET tx_hash = $2,
            raw_tx = $3,
            transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            tx_exec_receipt_uploaded_at = NULL,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(trade_no)
    .bind(tx_hash)
    .bind("{\"rebuilt\":true}")
    .execute(pool.as_ref())
    .await
    .expect("persist scan facts");
}

pub(super) fn collect_receipt_payload_json(req: &ApiCollectEntity, trade_no: &str) -> Value {
    serde_json::to_value(build_collect_tx_exec_receipt_payload(req, trade_no))
        .expect("serialize receipt payload")
}

pub(super) fn assert_collect_receipt_payload(
    payload_json: &Value,
    trade_no: &str,
    to_addr: &str,
    tx_hash: &str,
) {
    assert_eq!(payload_json["tradeNo"], trade_no);
    assert_eq!(payload_json["to"], to_addr);
    assert_eq!(payload_json["hash"], tx_hash);
    assert_eq!(payload_json["status"], "SUCCESS");
}

fn assert_collect_tx_exec_receipt_uploaded(rec: &ApiCollectEntity) {
    assert!(
        rec.tx_exec_receipt_uploaded_at.is_some(),
        "collect receipt upload should mark tx_exec_receipt_uploaded_at"
    );
}
