use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use alloy::primitives::U256;
use chrono::Utc;
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::mpsc;
use wallet_api::{
    domain::api_wallet::{RawTx, Tx},
    infrastructure::api_trans::{
        AddressLockManager, ShadowAdvancer, ShadowCollectCommand, ShadowCollectWorker,
    },
    testkit::{
        adapter_factory::{
            clear_test_transaction_adapter_override, set_test_transaction_adapter_override,
        },
        collect::scan_collect_intent_labels_once,
    },
};
use wallet_chain_interact::{
    BillResourceConsume, QueryTransactionResult, tron::operations::RawTransactionParams,
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool, SqliteContext,
    entities::{
        api_coin::ApiCoinData,
        api_collect::{ApiCollectEntity, ApiCollectStatus},
        asset_token_key::AssetTokenKey,
    },
    repositories::api_wallet::{coin::ApiCoinRepo, collect::ApiCollectRepo},
};
use wallet_types::chain::chain::ChainCode;

use crate::harness::next_unique_id;

const TRON_RECOVER_TX_HASH: &str =
    "6f2f3e7f5dbe46e7b8ff8d3c9b62df9b2b7b6f3e3c9d4a1d2f5d8e9f0a1b2c3d4";
const TRON_BACKFILL_TX_HASH: &str =
    "6f2f3e7f5dbe46e7b8ff8d3c9b62df9b2b7b6f3e3c9d4a1d2f5d8e9f0a1b2c3d5";

pub(super) struct CollectRecoveryFixture {
    trade_no: String,
    tx_hash: String,
}

impl CollectRecoveryFixture {
    pub(super) fn blockhash_rebuild() -> Self {
        Self {
            trade_no: "T_collect_blockhash_rebuild_refresh".to_string(),
            tx_hash: "old-hash".to_string(),
        }
    }

    pub(super) fn expired_tron_raw_probe() -> Self {
        Self {
            trade_no: format!("C_collect_recover_expired_raw_probe_{}", next_unique_id()),
            tx_hash: TRON_RECOVER_TX_HASH.to_string(),
        }
    }

    pub(super) fn tron_backfill() -> Self {
        Self {
            trade_no: format!("C_collect_recover_backfill_{}", next_unique_id()),
            tx_hash: TRON_BACKFILL_TX_HASH.to_string(),
        }
    }

    pub(super) fn broadcast_visible_pending() -> Self {
        Self {
            trade_no: format!("T_collect_recover_{}", next_unique_id()),
            tx_hash: "0xrecover".to_string(),
        }
    }
}

pub(super) struct LocalCollectRecoveryDb {
    _dir: TempDir,
    pool: ApiTransactionDbPool,
}

impl LocalCollectRecoveryDb {
    pub(super) async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx =
            SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_transaction.db"))
                .await
                .expect("init api_transaction.db");
        let pool = ctx.into_transaction_db_pool().expect("transaction pool");
        Self { _dir: dir, pool }
    }

    pub(super) async fn given_stale_blockhash_build(&self, fixture: &CollectRecoveryFixture) {
        ApiCollectRepo::upsert_api_collect(
            &self.pool,
            "uid",
            "collect",
            "from",
            "old-to",
            "1.12",
            "digest",
            "sol",
            Some("token".to_string()),
            "USDC",
            &fixture.trade_no,
            2,
            ApiCollectStatus::Init,
            1,
        )
        .await
        .expect("insert collect");

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
        .bind(&fixture.trade_no)
        .bind("{\"stale\":true}")
        .bind(&fixture.tx_hash)
        .bind(ApiCollectStatus::SendingTx)
        .execute(self.pool.as_ref())
        .await
        .expect("set stale build facts");
    }

    pub(super) async fn when_raw_tx_is_invalidated_for_rebuild(
        &self,
        fixture: &CollectRecoveryFixture,
    ) -> u64 {
        ApiCollectRepo::invalidate_raw_tx_for_rebuild(&self.pool, &fixture.trade_no, None)
            .await
            .expect("invalidate raw tx for rebuild")
    }

    pub(super) async fn then_stale_build_facts_are_cleared(
        &self,
        fixture: &CollectRecoveryFixture,
        invalidated: u64,
    ) {
        assert_eq!(invalidated, 1);

        let after_invalidate = self.load_collect(&fixture.trade_no).await;
        assert!(after_invalidate.raw_tx.is_none(), "stale raw_tx must be cleared");
        assert!(after_invalidate.tx_hash.is_none(), "stale tx_hash must be cleared");
        assert_eq!(
            after_invalidate.to_addr, "old-to",
            "rebuild invalidation must not invent a new execution address on its own"
        );
    }

    pub(super) async fn when_rebuilt_to_addr_is_persisted(
        &self,
        fixture: &CollectRecoveryFixture,
        to_addr: &str,
    ) {
        ApiCollectRepo::update_api_collect_to_addr(&self.pool, &fixture.trade_no, to_addr)
            .await
            .expect("persist rebuilt to_addr");
    }

    pub(super) async fn then_rebuilt_to_addr_is_persisted(
        &self,
        fixture: &CollectRecoveryFixture,
        to_addr: &str,
    ) {
        let rebuilt = self.load_collect(&fixture.trade_no).await;
        assert!(rebuilt.raw_tx.is_none(), "rebuild starts from cleared build facts");
        assert!(rebuilt.tx_hash.is_none(), "rebuild starts from cleared tx hash");
        assert_eq!(
            rebuilt.to_addr, to_addr,
            "next build must persist the latest strategy address before generating new raw_tx"
        );
    }

    pub(super) async fn given_broadcast_visible_pending_collect(
        &self,
        fixture: &CollectRecoveryFixture,
    ) {
        ApiCollectRepo::upsert_api_collect(
            &self.pool,
            "uid",
            "collect",
            "from-recover",
            "to-recover",
            "1.12",
            "digest",
            "eth",
            None,
            "USDC",
            &fixture.trade_no,
            2,
            ApiCollectStatus::SendingTx,
            1,
        )
        .await
        .expect("insert collect");

        sqlx::query(
            r#"
            UPDATE api_collect
            SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                raw_tx = '{"tx":true}',
                tx_hash = $2,
                last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = $1
            "#,
        )
        .bind(&fixture.trade_no)
        .bind(&fixture.tx_hash)
        .execute(self.pool.as_ref())
        .await
        .expect("seed recoverable collect row");
    }

    pub(super) async fn when_collect_scanner_runs(&self) -> Vec<String> {
        scan_collect_intent_labels_once(self.pool.clone())
            .await
            .expect("scanner round should succeed")
    }

    pub(super) fn then_scanner_emits_recover_only(&self, labels: Vec<String>) {
        assert!(
            labels.iter().any(|label| label == "RecoverTx"),
            "broadcast-visible pending collect row must emit RecoverTx"
        );
        assert!(
            labels.iter().all(|label| label != "BuildTx"),
            "recoverable row should not re-enter build"
        );
        assert!(
            labels.iter().all(|label| label != "UploadServiceFee"),
            "recoverable row should not go back to fee upload"
        );
    }

    pub(super) async fn then_recoverable_row_stays_pending(
        &self,
        fixture: &CollectRecoveryFixture,
    ) {
        let persisted_after = self.load_collect(&fixture.trade_no).await;
        assert_eq!(persisted_after.tx_hash.as_deref(), Some(fixture.tx_hash.as_str()));
        assert!(persisted_after.transaction_time.is_none());
    }

    async fn load_collect(&self, trade_no: &str) -> ApiCollectEntity {
        ApiCollectRepo::get_api_collect_by_trade_no(&self.pool, trade_no)
            .await
            .expect("load collect")
    }
}

pub(super) struct ShadowCollectRecoveryScenario {
    _dir: TempDir,
    collect_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
    query_count: Option<Arc<AtomicUsize>>,
    _adapter_guard: Option<TronRecoverProbeGuard>,
}

impl ShadowCollectRecoveryScenario {
    pub(super) async fn new() -> Self {
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

    pub(super) fn given_chain_probe_confirms_tx(&mut self, fixture: &CollectRecoveryFixture) {
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

    pub(super) fn given_chain_query_clears_hash_then_confirms(
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

    pub(super) async fn given_expired_raw_tx_collect(&self, fixture: &CollectRecoveryFixture) {
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

    pub(super) async fn given_recoverable_collect_with_tx_hash(
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

    pub(super) async fn when_recover_runs(&self, fixture: &CollectRecoveryFixture) {
        let worker = build_shadow_collect_worker_from_pools(
            self.collect_pool.clone(),
            self.core_pool.clone(),
        );
        worker
            .handle(ShadowCollectCommand::Recover(fixture.trade_no.clone()))
            .await
            .expect("recover command should succeed");
    }

    pub(super) fn then_chain_was_queried_once(&self) {
        let query_count = self.query_count.as_ref().expect("probe query count installed");
        assert_eq!(query_count.load(Ordering::Relaxed), 1, "recover must query chain first");
    }

    pub(super) async fn then_expired_raw_tx_is_confirmed_without_rebuild(
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

    pub(super) async fn then_tx_hash_is_backfilled_and_receipt_upload_needed(
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

#[derive(Clone)]
struct CollectTronRecoverProbeAdapter {
    query_count: Arc<AtomicUsize>,
    query_hook: Option<Arc<dyn Fn() + Send + Sync>>,
    tx_hash: String,
    transaction_fee: f64,
    resource_consume: String,
    transaction_time_ms: u128,
    block_height: u128,
}

#[async_trait::async_trait]
impl Tx for CollectTronRecoverProbeAdapter {
    async fn account_resource(
        &self,
        _owner_address: &str,
    ) -> Result<
        wallet_chain_interact::tron::protocol::account::AccountResourceDetail,
        wallet_api::error::service::ServiceError,
    > {
        unimplemented!("not used in collect recover probe")
    }

    async fn balance_token_key(
        &self,
        _addr: &str,
        _token: AssetTokenKey,
    ) -> Result<U256, wallet_chain_interact::Error> {
        Ok(U256::ZERO)
    }

    async fn nonce(&self, _addr: &str) -> Result<u64, wallet_api::error::service::ServiceError> {
        Ok(0)
    }

    async fn block_num(&self) -> Result<u64, wallet_chain_interact::Error> {
        Ok(self.block_height as u64)
    }

    async fn query_tx_res(
        &self,
        _hash: &str,
    ) -> Result<Option<QueryTransactionResult>, wallet_chain_interact::Error> {
        self.query_count.fetch_add(1, Ordering::Relaxed);
        if let Some(hook) = &self.query_hook {
            hook();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(Some(QueryTransactionResult::new(
            self.tx_hash.clone(),
            self.transaction_fee,
            self.resource_consume.clone(),
            self.transaction_time_ms,
            2,
            self.block_height,
        )))
    }

    async fn token_symbol(&self, _token: &str) -> Result<String, wallet_chain_interact::Error> {
        Ok("TRX".to_string())
    }

    async fn token_name(&self, _token: &str) -> Result<String, wallet_chain_interact::Error> {
        Ok("Tron".to_string())
    }

    async fn decimals(&self, _token: &str) -> Result<u8, wallet_chain_interact::Error> {
        Ok(6)
    }

    async fn black_address(
        &self,
        _token: &str,
        _owner: &str,
    ) -> Result<bool, wallet_api::error::service::ServiceError> {
        Ok(false)
    }

    async fn transfer(
        &self,
        _params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<wallet_api::domain::chain::TransferResp, wallet_api::error::service::ServiceError>
    {
        unimplemented!("not used in collect recover probe")
    }

    async fn estimate_fee(
        &self,
        _req: wallet_api::request::api_wallet::trans::ApiBaseTransferReq,
        _main_symbol: &str,
    ) -> Result<String, wallet_api::error::service::ServiceError> {
        Ok(json!({
            "estimateFee": {
                "amount": "0.1",
                "currency": "USD",
                "unitPrice": 0.0,
                "fiatValue": 0.0
            }
        })
        .to_string())
    }

    async fn build_transfer_raw(
        &self,
        _params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<(String, RawTx, String), wallet_api::error::service::ServiceError> {
        unimplemented!("not used in collect recover probe")
    }

    async fn broadcast_transfer(
        &self,
        _raw: RawTx,
    ) -> Result<wallet_api::domain::chain::TransferResp, wallet_api::error::service::ServiceError>
    {
        unimplemented!("not used in collect recover probe")
    }
}

struct TronRecoverProbeGuard {
    chain_code: String,
}

impl Drop for TronRecoverProbeGuard {
    fn drop(&mut self) {
        clear_test_transaction_adapter_override(&self.chain_code);
    }
}

fn install_collect_tron_recover_probe_adapter(
    query_count: Arc<AtomicUsize>,
    query_hook: Option<Arc<dyn Fn() + Send + Sync>>,
    tx_hash: &str,
    transaction_fee: f64,
    resource_consume: &str,
    transaction_time_ms: u128,
    block_height: u128,
) -> TronRecoverProbeGuard {
    let chain_code = ChainCode::Tron.to_string();
    let adapter = Arc::new(CollectTronRecoverProbeAdapter {
        query_count,
        query_hook,
        tx_hash: tx_hash.to_string(),
        transaction_fee,
        resource_consume: resource_consume.to_string(),
        transaction_time_ms,
        block_height,
    });
    let tx_adapter: Arc<dyn Tx + Send + Sync> = adapter;
    set_test_transaction_adapter_override(&chain_code, tx_adapter);
    TronRecoverProbeGuard { chain_code }
}

fn expired_tron_raw_tx_json(expiration_ms: i64) -> String {
    let raw = RawTransactionParams {
        tx_id: "expired-tron-tx".to_string(),
        raw_data: json!({
            "expiration": expiration_ms,
            "timestamp": expiration_ms.saturating_sub(1_000),
        })
        .to_string(),
        raw_data_hex: "0a00".to_string(),
        signature: vec![],
    };
    let bill = BillResourceConsume::new_tron(0, 0);
    serde_json::to_string(&RawTx::Tron(raw, bill, "0".to_string()))
        .expect("serialize expired tron raw tx")
}

async fn ensure_sol_main_coin(pool: &ApiWalletDbPool) {
    let now = Utc::now();
    let coin = ApiCoinData::new(
        Some("Solana".to_string()),
        "SOL",
        "sol",
        AssetTokenKey::Native,
        Some("0".to_string()),
        None,
        9,
        1,
        1,
        1,
        now,
        Some(now),
    );
    ApiCoinRepo::upsert_multi_coin(pool, vec![coin]).await.expect("seed sol main coin");
}

fn build_shadow_collect_worker_from_pools(
    collect_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
) -> ShadowCollectWorker {
    let (intent_tx, _intent_rx) = mpsc::channel(1);
    let advancer = Arc::new(ShadowAdvancer::new(collect_pool.clone(), intent_tx, None));

    ShadowCollectWorker::new(collect_pool, core_pool, Arc::new(AddressLockManager::new()), advancer)
}

async fn seed_tron_collect(pool: &ApiTransactionDbPool, fixture: &CollectRecoveryFixture) {
    ApiCollectRepo::upsert_api_collect(
        pool,
        "uid",
        "collect",
        "from-tron",
        "to-tron",
        "1.1325",
        "digest",
        "tron",
        Some("TXYZopYRdj2D9XRtbG411XZZ3kM5VkAeBf".to_string()),
        "USDT",
        &fixture.trade_no,
        2,
        ApiCollectStatus::SendingTx,
        0,
    )
    .await
    .expect("seed tron collect");
}
