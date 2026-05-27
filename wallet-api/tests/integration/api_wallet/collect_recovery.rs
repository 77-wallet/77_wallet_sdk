use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use alloy::primitives::U256;
use chrono::Utc;
use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;
use tokio::sync::mpsc;
use wallet_api::{
    domain::api_wallet::{RawTx, Tx},
    infrastructure::api_trans::{
        AddressLockManager, ShadowAdvancer, ShadowCollectCommand, ShadowCollectWorker,
    },
    test::collect::scan_collect_intent_labels_once,
    test_support::adapter_factory::{
        clear_test_transaction_adapter_override, set_test_transaction_adapter_override,
    },
};
use wallet_chain_interact::{
    BillResourceConsume, QueryTransactionResult, tron::operations::RawTransactionParams,
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool, SqliteContext,
    entities::{
        api_coin::ApiCoinData, api_collect::ApiCollectStatus, asset_token_key::AssetTokenKey,
    },
    repositories::api_wallet::{coin::ApiCoinRepo, collect::ApiCollectRepo},
};
use wallet_types::chain::chain::ChainCode;

use crate::harness::next_unique_id;

struct LocalCollectDb {
    _dir: TempDir,
    pool: ApiTransactionDbPool,
}

impl LocalCollectDb {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx =
            SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_transaction.db"))
                .await
                .expect("init api_transaction.db");
        let pool = ctx.into_transaction_db_pool().expect("transaction pool");
        Self { _dir: dir, pool }
    }
}

struct LocalShadowCollectDb {
    _dir: TempDir,
    collect_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
}

impl LocalShadowCollectDb {
    async fn new() -> Self {
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

        Self { _dir: dir, collect_pool, core_pool }
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
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
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

#[tokio::test]
async fn collect_blockhash_rebuild_clears_stale_build_facts_and_persists_new_to_addr() {
    let db = LocalCollectDb::new().await;
    let trade_no = "T_collect_blockhash_rebuild_refresh";

    ApiCollectRepo::upsert_api_collect(
        &db.pool,
        "uid",
        "collect",
        "from",
        "old-to",
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
        "USDC",
        trade_no,
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
    .bind(trade_no)
    .bind("{\"stale\":true}")
    .bind("old-hash")
    .bind(ApiCollectStatus::SendingTx)
    .execute(db.pool.as_ref())
    .await
    .expect("set stale build facts");

    let invalidated = ApiCollectRepo::invalidate_raw_tx_for_rebuild(&db.pool, trade_no, None)
        .await
        .expect("invalidate raw tx for rebuild");
    assert_eq!(invalidated, 1);

    let after_invalidate = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("load collect after invalidate");
    assert!(after_invalidate.raw_tx.is_none(), "stale raw_tx must be cleared");
    assert!(after_invalidate.tx_hash.is_none(), "stale tx_hash must be cleared");
    assert_eq!(
        after_invalidate.to_addr, "old-to",
        "rebuild invalidation must not invent a new execution address on its own"
    );

    ApiCollectRepo::update_api_collect_to_addr(&db.pool, trade_no, "new-to")
        .await
        .expect("persist rebuilt to_addr");

    let rebuilt = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("load rebuilt collect");
    assert!(rebuilt.raw_tx.is_none(), "rebuild starts from cleared build facts");
    assert!(rebuilt.tx_hash.is_none(), "rebuild starts from cleared tx hash");
    assert_eq!(
        rebuilt.to_addr, "new-to",
        "next build must persist the latest strategy address before generating new raw_tx"
    );
}

#[tokio::test]
#[serial]
async fn collect_recover_queries_chain_before_any_expired_raw_rebuild_invalidation() {
    let db = LocalShadowCollectDb::new().await;
    let collect_pool = db.collect_pool.clone();
    let trade_no = format!("C_collect_recover_expired_raw_probe_{}", next_unique_id());
    let tx_hash = "6f2f3e7f5dbe46e7b8ff8d3c9b62df9b2b7b6f3e3c9d4a1d2f5d8e9f0a1b2c3d4";
    let query_count = Arc::new(AtomicUsize::new(0));
    let _adapter_guard = install_collect_tron_recover_probe_adapter(
        query_count.clone(),
        None,
        tx_hash,
        0.25,
        r#"{"net_used":0,"energy_used":0}"#,
        1_700_000_000_000,
        99,
    );

    ApiCollectRepo::upsert_api_collect(
        &collect_pool,
        "uid",
        "collect",
        "from-tron",
        "to-tron",
        "1.1325",
        "digest",
        "tron",
        Some("TXYZopYRdj2D9XRtbG411XZZ3kM5VkAeBf".to_string()),
        "USDT",
        &trade_no,
        2,
        ApiCollectStatus::SendingTx,
        0,
    )
    .await
    .expect("seed tron collect");

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
    .bind(&trade_no)
    .bind(&expired_raw_tx)
    .bind(tx_hash)
    .bind(ApiCollectStatus::SendingTx)
    .execute(collect_pool.as_ref())
    .await
    .expect("seed expired raw tx facts");

    let worker = build_shadow_collect_worker_from_pools(collect_pool.clone(), db.core_pool.clone());
    worker
        .handle(ShadowCollectCommand::Recover(trade_no.clone()))
        .await
        .expect("recover command should succeed");

    assert_eq!(query_count.load(Ordering::Relaxed), 1, "recover must query chain first");

    let after = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("reload collect after recover");
    assert!(after.transaction_time.is_some(), "recover must persist chain confirmation");
    assert!(after.last_broadcast_at.is_some(), "broadcast evidence must be preserved");
    assert!(
        after.raw_tx.is_some(),
        "expired raw tx must not be invalidated before final confirmation"
    );
}

#[tokio::test]
#[serial]
async fn collect_recover_backfills_missing_tx_hash_before_receipt_upload() {
    let db = LocalShadowCollectDb::new().await;
    let collect_pool = db.collect_pool.clone();
    let trade_no = format!("C_collect_recover_backfill_{}", next_unique_id());
    let tx_hash = "6f2f3e7f5dbe46e7b8ff8d3c9b62df9b2b7b6f3e3c9d4a1d2f5d8e9f0a1b2c3d5";

    ApiCollectRepo::upsert_api_collect(
        &collect_pool,
        "uid",
        "collect",
        "from-tron",
        "to-tron",
        "1.1325",
        "digest",
        "tron",
        Some("TXYZopYRdj2D9XRtbG411XZZ3kM5VkAeBf".to_string()),
        "USDT",
        &trade_no,
        2,
        ApiCollectStatus::SendingTx,
        0,
    )
    .await
    .expect("seed collect");

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
    .bind(tx_hash)
    .bind(&trade_no)
    .execute(collect_pool.as_ref())
    .await
    .expect("seed recoverable collect row");

    let clear_trade_no = trade_no.clone();
    let clear_pool = collect_pool.clone();
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
    let _adapter_guard = install_collect_tron_recover_probe_adapter(
        Arc::new(AtomicUsize::new(0)),
        Some(query_hook),
        tx_hash,
        0.25,
        r#"{"net_used":0,"energy_used":0}"#,
        1_700_000_000_000,
        99,
    );

    let worker = build_shadow_collect_worker_from_pools(collect_pool.clone(), db.core_pool.clone());
    worker
        .handle(ShadowCollectCommand::Recover(trade_no.clone()))
        .await
        .expect("recover command should succeed");

    let after = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("reload collect after recover");
    assert_eq!(after.tx_hash.as_deref(), Some(tx_hash));
    assert!(after.transaction_time.is_some());

    let records = ApiCollectRepo::scan_need_tx_exec_receipt_upload(&collect_pool, 10_000)
        .await
        .expect("scan need tx exec receipt upload");
    assert!(
        records.iter().any(|r| r.trade_no == trade_no),
        "recovered collect with backfilled hash must enter receipt upload scan"
    );
}

#[tokio::test]
async fn collect_scanner_recovers_broadcast_visible_pending_result() {
    let db = LocalCollectDb::new().await;
    let trade_no = format!("T_collect_recover_{}", next_unique_id());

    ApiCollectRepo::upsert_api_collect(
        &db.pool,
        "uid",
        "collect",
        "from-recover",
        "to-recover",
        "1.12",
        "digest",
        "eth",
        None,
        "USDC",
        &trade_no,
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
            tx_hash = '0xrecover',
            last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = ?
        "#,
    )
    .bind(&trade_no)
    .execute(db.pool.as_ref())
    .await
    .expect("seed recoverable collect row");

    let labels = scan_collect_intent_labels_once(db.pool.clone())
        .await
        .expect("scanner round should succeed");

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

    let persisted_after = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, &trade_no)
        .await
        .expect("load collect after scanner round");
    assert_eq!(persisted_after.tx_hash.as_deref(), Some("0xrecover"));
    assert!(persisted_after.transaction_time.is_none());
}
