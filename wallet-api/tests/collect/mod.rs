#![cfg(feature = "integration-tests")]

#[path = "../common/mod.rs"]
mod common;

use alloy::primitives::U256;
use chrono::Utc;
use common::{SMOKE_WALLET_PASSWORD, upsert_wallet};
use serde_json::json;
use serial_test::serial;
use sqlx;
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    net::{Shutdown, TcpListener},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};
use tempfile::TempDir;
use tokio::sync::{OnceCell, mpsc};
use wallet_api::{
    ApiWalletBackend,
    dirs::Dirs,
    domain::api_wallet::{RawTx, Tx},
    error::business::{
        BusinessError,
        chain::{ChainError, InsufficientBalanceDetail},
    },
    infrastructure::api_trans::{
        AddressLockManager, ShadowAdvancer, ShadowCollectCommand, ShadowCollectWorker,
    },
    manager::WalletManager,
    messaging::notify::FrontendNotifyEvent,
    test::collect::{
        build_collect_tx_exec_receipt_payload, scan_and_dispatch_collect_tx_exec_receipt_once,
        scan_collect_intent_labels_once, upload_collect_service_fee_via_worker,
        upload_collect_tx_exec_receipt_via_backend, upload_collect_tx_exec_receipt_via_worker,
    },
    test_support::{
        adapter_factory::{
            clear_test_transaction_adapter_override, set_test_transaction_adapter_override,
        },
        collect::shadow_collect_check_fee,
    },
};
use wallet_chain_interact::{
    BillResourceConsume, QueryTransactionResult, tron::operations::RawTransactionParams,
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool, SqliteContext,
    entities::{
        api_account::CreateApiAccountVo,
        api_coin::ApiCoinData,
        api_collect::{ApiCollectEntity, ApiCollectStatus},
        api_wallet::ApiWalletType,
        api_withdraw::ApiWithdrawStatus,
        api_withdraw_strategy::ApiWithdrawStrategyEntity,
        api_withdraw_strategy_chain_config::ApiWithdrawStrategyChainConfigEntity,
        asset_token_key::AssetTokenKey,
    },
    repositories::api_wallet::{
        account::ApiAccountRepo, coin::ApiCoinRepo, collect::ApiCollectRepo, wallet::ApiWalletRepo,
        withdraw::ApiWithdrawRepo, withdraw_strategy::ApiWithdrawStrategyRepo,
        withdraw_strategy_chain_config::ApiWithdrawStrategyChainConfigRepo,
    },
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::{
    request::{
        KeysInitReq,
        api_wallet::wallet::{
            AppIdImportRechargeWalletReq, AppIdImportReq, AppIdUidUsageReq, BindAppIdReq,
        },
    },
    response_vo::api_wallet::wallet::{AppIdUidUsageRes, KeysUidCheckRes, QueryUidBindInfoRes},
};
use wallet_types::chain::chain::ChainCode;

const TEST_SN: &str = "collect-worker-test-sn";
const TEST_DEVICE_TYPE: &str = "ANDROID";
const TEST_PUB_KEY: &str = r#"-----BEGIN PUBLIC KEY-----
MFYwEAYHKoZIzj0CAQYFK4EEAAoDQgAEWDZNP0ClbeWJey9hBr2rsjSayQEBywnv
ZXi0RberQCAp+06fOjvr+jZI5qwYGglmMkGJw49tbni6qgm4QNV6WQ==
-----END PUBLIC KEY-----"#;
static WORKER_ENV: OnceCell<WorkerTestEnv> = OnceCell::const_new();
static UNIQUE_ID: AtomicU64 = AtomicU64::new(1);

struct TestFundsDb {
    _dir: TempDir,
    pool: ApiTransactionDbPool,
}

impl TestFundsDb {
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

#[derive(Clone, Debug)]
struct CapturedHttpRequest {
    path: String,
    body: String,
}

#[derive(Default)]
struct MockBackendState {
    requests: VecDeque<CapturedHttpRequest>,
}

#[derive(Clone, Default)]
struct MockBackendRecorder {
    state: Arc<Mutex<MockBackendState>>,
}

impl MockBackendRecorder {
    fn push(&self, req: CapturedHttpRequest) {
        let mut state = self.state.lock().expect("mock backend lock poisoned");
        state.requests.push_back(req);
    }

    fn pop(&self) -> Option<CapturedHttpRequest> {
        let mut state = self.state.lock().expect("mock backend lock poisoned");
        state.requests.pop_front()
    }

    fn reset(&self) {
        let mut state = self.state.lock().expect("mock backend lock poisoned");
        state.requests.clear();
    }

    fn snapshot(&self) -> Vec<CapturedHttpRequest> {
        let state = self.state.lock().expect("mock backend lock poisoned");
        state.requests.iter().cloned().collect()
    }
}

async fn pop_request_with_retry(recorder: &MockBackendRecorder) -> Option<CapturedHttpRequest> {
    for _ in 0..20 {
        if let Some(req) = recorder.pop() {
            return Some(req);
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    None
}

#[derive(Default)]
struct NoopApiWalletBackend;

#[async_trait::async_trait]
impl ApiWalletBackend for NoopApiWalletBackend {
    async fn wallet_bind_appid(
        &self,
        _req: BindAppIdReq,
    ) -> Result<(), wallet_api::error::service::ServiceError> {
        Ok(())
    }

    async fn init_api_wallet(
        &self,
        _req: AppIdImportReq,
    ) -> Result<(), wallet_api::error::service::ServiceError> {
        Ok(())
    }

    async fn old_keys_init(
        &self,
        _req: KeysInitReq,
    ) -> Result<(), wallet_api::error::service::ServiceError> {
        Ok(())
    }

    async fn appid_import(
        &self,
        _req: AppIdImportReq,
    ) -> Result<(), wallet_api::error::service::ServiceError> {
        Ok(())
    }

    async fn appid_import_recharge_wallet(
        &self,
        _req: AppIdImportRechargeWalletReq,
    ) -> Result<(), wallet_api::error::service::ServiceError> {
        Ok(())
    }

    async fn keys_uid_check(
        &self,
        _uid: &str,
    ) -> Result<KeysUidCheckRes, wallet_api::error::service::ServiceError> {
        Err(wallet_api::error::service::ServiceError::System(
            wallet_api::error::system::SystemError::Internal("noop".to_string()),
        ))
    }

    async fn query_uid_bind_info(
        &self,
        _uid: &str,
    ) -> Result<QueryUidBindInfoRes, wallet_api::error::service::ServiceError> {
        Err(wallet_api::error::service::ServiceError::System(
            wallet_api::error::system::SystemError::Internal("noop".to_string()),
        ))
    }

    async fn appid_uid_usage(
        &self,
        _req: AppIdUidUsageReq,
    ) -> Result<AppIdUidUsageRes, wallet_api::error::service::ServiceError> {
        Err(wallet_api::error::service::ServiceError::System(
            wallet_api::error::system::SystemError::Internal("noop".to_string()),
        ))
    }
}

#[derive(Clone)]
struct CollectSolTestAdapter {
    recipient_missing: bool,
    force_fee_insufficient: bool,
    balance: u64,
    fee: f64,
}

#[async_trait::async_trait]
impl Tx for CollectSolTestAdapter {
    async fn account_resource(
        &self,
        _owner_address: &str,
    ) -> Result<
        wallet_chain_interact::tron::protocol::account::AccountResourceDetail,
        wallet_api::error::service::ServiceError,
    > {
        unimplemented!("not used in collect fee checks")
    }

    async fn balance_token_key(
        &self,
        _addr: &str,
        _token: AssetTokenKey,
    ) -> Result<U256, wallet_chain_interact::Error> {
        Ok(U256::from(self.balance))
    }

    async fn nonce(&self, _addr: &str) -> Result<u64, wallet_api::error::service::ServiceError> {
        Ok(0)
    }

    async fn block_num(&self) -> Result<u64, wallet_chain_interact::Error> {
        Ok(0)
    }

    async fn query_tx_res(
        &self,
        _hash: &str,
    ) -> Result<Option<wallet_chain_interact::QueryTransactionResult>, wallet_chain_interact::Error>
    {
        Ok(None)
    }

    async fn token_symbol(&self, _token: &str) -> Result<String, wallet_chain_interact::Error> {
        Ok("SOL".to_string())
    }

    async fn token_name(&self, _token: &str) -> Result<String, wallet_chain_interact::Error> {
        Ok("Solana".to_string())
    }

    async fn decimals(&self, _token: &str) -> Result<u8, wallet_chain_interact::Error> {
        Ok(9)
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
        unimplemented!("not used in collect fee checks")
    }

    async fn estimate_fee(
        &self,
        req: wallet_api::request::api_wallet::trans::ApiBaseTransferReq,
        _main_symbol: &str,
    ) -> Result<String, wallet_api::error::service::ServiceError> {
        if self.force_fee_insufficient {
            return Err(wallet_api::error::service::ServiceError::Business(BusinessError::Chain(
                ChainError::InsufficientFeeBalance,
            )));
        }

        if self.recipient_missing {
            return Err(wallet_api::error::service::ServiceError::Business(
                BusinessError::Chain(ChainError::insufficient_balance_with_detail(
                    InsufficientBalanceDetail::new()
                        .from_addr(req.from)
                        .to_addr(req.to)
                        .chain_code("sol".to_string())
                        .value(req.value)
                        .balance(self.balance.to_string())
                        .need("990880".to_string())
                        .reason(
                            "recipient account is not initialized and transfer amount is below rent-exempt minimum",
                        ),
                )),
            ));
        }

        Ok(json!({
            "estimateFee": {
                "amount": format!("{}", self.fee),
                "currency": "USD",
                "unitPrice": 0.0,
                "fiatValue": 0.0
            }
        })
        .to_string())
    }

    async fn estimate_fee_without_balance_check(
        &self,
        _req: wallet_api::request::api_wallet::trans::ApiBaseTransferReq,
        _main_symbol: &str,
    ) -> Result<String, wallet_api::error::service::ServiceError> {
        Ok(json!({
            "estimateFee": {
                "amount": format!("{}", self.fee),
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
        unimplemented!("not used in collect fee checks")
    }

    async fn broadcast_transfer(
        &self,
        _raw: RawTx,
    ) -> Result<wallet_api::domain::chain::TransferResp, wallet_api::error::service::ServiceError>
    {
        unimplemented!("not used in collect fee checks")
    }
}

#[derive(Clone)]
struct CollectEthTestAdapter {
    balance_wei: U256,
    fee_amount: f64,
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

impl CollectEthTestAdapter {
    fn fee_json(&self) -> String {
        json!({
            "default": "propose",
            "data": [{
                "type": "propose",
                "estimateFee": {
                    "amount": format!("{}", self.fee_amount),
                    "currency": "USD",
                    "unitPrice": 0.0,
                    "fiatValue": 0.0
                },
                "maxFee": {
                    "amount": format!("{}", self.fee_amount * 1.2),
                    "currency": "USD",
                    "unitPrice": 0.0,
                    "fiatValue": 0.0
                },
                "feeSetting": {
                    "gasLimit": 23100,
                    "baseFee": "1000000000",
                    "priorityFee": "1000000000",
                    "maxFeePerGas": "2000000000"
                }
            }]
        })
        .to_string()
    }
}

#[async_trait::async_trait]
impl Tx for CollectEthTestAdapter {
    async fn account_resource(
        &self,
        _owner_address: &str,
    ) -> Result<
        wallet_chain_interact::tron::protocol::account::AccountResourceDetail,
        wallet_api::error::service::ServiceError,
    > {
        unimplemented!("not used in collect fee checks")
    }

    async fn balance_token_key(
        &self,
        _addr: &str,
        _token: AssetTokenKey,
    ) -> Result<U256, wallet_chain_interact::Error> {
        Ok(self.balance_wei)
    }

    async fn nonce(&self, _addr: &str) -> Result<u64, wallet_api::error::service::ServiceError> {
        Ok(0)
    }

    async fn block_num(&self) -> Result<u64, wallet_chain_interact::Error> {
        Ok(0)
    }

    async fn query_tx_res(
        &self,
        _hash: &str,
    ) -> Result<Option<wallet_chain_interact::QueryTransactionResult>, wallet_chain_interact::Error>
    {
        Ok(None)
    }

    async fn token_symbol(&self, _token: &str) -> Result<String, wallet_chain_interact::Error> {
        Ok("ETH".to_string())
    }

    async fn token_name(&self, _token: &str) -> Result<String, wallet_chain_interact::Error> {
        Ok("Ethereum".to_string())
    }

    async fn decimals(&self, _token: &str) -> Result<u8, wallet_chain_interact::Error> {
        Ok(18)
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
        unimplemented!("not used in collect fee checks")
    }

    async fn estimate_fee(
        &self,
        _req: wallet_api::request::api_wallet::trans::ApiBaseTransferReq,
        _main_symbol: &str,
    ) -> Result<String, wallet_api::error::service::ServiceError> {
        Ok(self.fee_json())
    }

    async fn build_transfer_raw(
        &self,
        _params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<(String, RawTx, String), wallet_api::error::service::ServiceError> {
        unimplemented!("not used in collect fee checks")
    }

    async fn broadcast_transfer(
        &self,
        _raw: RawTx,
    ) -> Result<wallet_api::domain::chain::TransferResp, wallet_api::error::service::ServiceError>
    {
        unimplemented!("not used in collect fee checks")
    }
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

struct TestAdapterGuard {
    chain_code: String,
}

impl Drop for TestAdapterGuard {
    fn drop(&mut self) {
        clear_test_transaction_adapter_override(&self.chain_code);
    }
}

fn install_collect_test_adapter(recipient_missing: bool, balance: u64) -> TestAdapterGuard {
    let chain_code = ChainCode::Solana.to_string();
    let adapter = Arc::new(CollectSolTestAdapter {
        recipient_missing,
        force_fee_insufficient: false,
        balance,
        fee: 0.000015,
    });
    let tx_adapter: Arc<dyn Tx + Send + Sync> = adapter;
    set_test_transaction_adapter_override(&chain_code, tx_adapter);
    TestAdapterGuard { chain_code }
}

fn install_collect_test_adapter_fee_shortage(
    recipient_missing: bool,
    balance: u64,
) -> TestAdapterGuard {
    let chain_code = ChainCode::Solana.to_string();
    let adapter = Arc::new(CollectSolTestAdapter {
        recipient_missing,
        force_fee_insufficient: true,
        balance,
        fee: 0.000015,
    });
    let tx_adapter: Arc<dyn Tx + Send + Sync> = adapter;
    set_test_transaction_adapter_override(&chain_code, tx_adapter);
    TestAdapterGuard { chain_code }
}

struct EthAdapterGuard {
    chain_code: String,
}

impl Drop for EthAdapterGuard {
    fn drop(&mut self) {
        clear_test_transaction_adapter_override(&self.chain_code);
    }
}

fn install_collect_eth_test_adapter(balance_wei: U256, fee_amount: f64) -> EthAdapterGuard {
    let chain_code = ChainCode::Ethereum.to_string();
    let adapter = Arc::new(CollectEthTestAdapter { balance_wei, fee_amount });
    let tx_adapter: Arc<dyn Tx + Send + Sync> = adapter;
    set_test_transaction_adapter_override(&chain_code, tx_adapter);
    EthAdapterGuard { chain_code }
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

async fn build_shadow_collect_worker(env: &WorkerTestEnv) -> ShadowCollectWorker {
    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    ensure_sol_main_coin(&core_pool).await;
    let (intent_tx, _intent_rx) = mpsc::channel(1);
    let advancer = Arc::new(ShadowAdvancer::new(collect_pool.clone(), intent_tx, None));

    ShadowCollectWorker::new(collect_pool, core_pool, Arc::new(AddressLockManager::new()), advancer)
}

async fn ensure_eth_main_coin(pool: &ApiWalletDbPool) {
    let now = Utc::now();
    let coin = ApiCoinData::new(
        Some("Ethereum".to_string()),
        "ETH",
        "eth",
        AssetTokenKey::Native,
        Some("0".to_string()),
        None,
        18,
        1,
        1,
        1,
        now,
        Some(now),
    );
    ApiCoinRepo::upsert_multi_coin(pool, vec![coin]).await.expect("seed eth main coin");
}

async fn build_eth_shadow_collect_worker(env: &WorkerTestEnv) -> ShadowCollectWorker {
    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    ensure_eth_main_coin(&core_pool).await;
    let (intent_tx, _intent_rx) = mpsc::channel(1);
    let advancer = Arc::new(ShadowAdvancer::new(collect_pool.clone(), intent_tx, None));

    ShadowCollectWorker::new(collect_pool, core_pool, Arc::new(AddressLockManager::new()), advancer)
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

async fn seed_collect_order(
    pool: &ApiTransactionDbPool,
    trade_no: &str,
    to_addr: &str,
) -> ApiCollectEntity {
    ApiCollectRepo::upsert_api_collect(
        pool,
        "uid",
        "collect",
        "from-sol",
        to_addr,
        "0.000015",
        "digest",
        "sol",
        None,
        "SOL",
        trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

    ApiCollectRepo::get_api_collect_by_trade_no(pool, trade_no).await.expect("load collect")
}

async fn seed_eth_collect_order(
    pool: &ApiTransactionDbPool,
    trade_no: &str,
    from_addr: &str,
    to_addr: &str,
    value: &str,
) -> ApiCollectEntity {
    ApiCollectRepo::upsert_api_collect(
        pool,
        "uid",
        "collect",
        from_addr,
        to_addr,
        value,
        "digest",
        "eth",
        None,
        "ETH",
        trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

    ApiCollectRepo::get_api_collect_by_trade_no(pool, trade_no).await.expect("load collect")
}

async fn seed_wallet(
    db_dir: &Path,
    uid: &str,
    wallet_name: &str,
    wallet_type: ApiWalletType,
) -> String {
    let pool = open_api_wallet_pool(db_dir).await;
    let address = format!("0xwallet{:016x}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let seed_enc = wallet_api::test::seed::encrypt_seed(SMOKE_WALLET_PASSWORD, b"seed").await;
    ApiWalletRepo::upsert(
        &pool,
        uid,
        wallet_name,
        &address,
        b"phrase",
        &seed_enc,
        wallet_type,
        None,
        TEST_SN,
        0,
    )
    .await
    .expect("seed wallet");
    address
}

struct WorkerTestEnv {
    _manager: WalletManager,
    backend_url: String,
    db_dir: PathBuf,
    recorder: MockBackendRecorder,
}

fn start_mock_backend_server() -> io::Result<(String, MockBackendRecorder)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let recorder = MockBackendRecorder::default();
    let recorder_clone = recorder.clone();

    std::thread::spawn(move || {
        loop {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let recorder = recorder_clone.clone();
            std::thread::spawn(move || {
                let mut header_buf = Vec::new();
                let mut temp = [0u8; 1024];
                let header_end;
                loop {
                    let n = match stream.read(&mut temp) {
                        Ok(0) => return,
                        Ok(n) => n,
                        Err(_) => return,
                    };
                    header_buf.extend_from_slice(&temp[..n]);
                    if let Some(pos) = header_buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = pos + 4;
                        break;
                    }
                }

                let header_text = String::from_utf8_lossy(&header_buf[..header_end]);
                let path = header_text
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or_default()
                    .to_string();
                let content_length = header_text
                    .lines()
                    .find_map(|line| {
                        let lower = line.to_ascii_lowercase();
                        lower
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);

                let mut body = header_buf[header_end..].to_vec();
                while body.len() < content_length {
                    let n = match stream.read(&mut temp) {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(_) => return,
                    };
                    body.extend_from_slice(&temp[..n]);
                }

                recorder.push(CapturedHttpRequest {
                    path,
                    body: String::from_utf8_lossy(&body).to_string(),
                });

                let response_body = r#"{"success":true,"code":"200","msg":"ok","data":null}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
            });
        }
    });

    Ok((format!("http://{}", addr), recorder))
}

fn create_test_root_dir() -> PathBuf {
    let pid = std::process::id();
    let id = UNIQUE_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("wallet_api_collect_worker_{pid}_{id}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create test root");
    root
}

async fn open_api_wallet_pool(db_dir: &Path) -> ApiWalletDbPool {
    let sqlite = SqliteContext::new(&db_dir.to_string_lossy(), Some("api_wallet.db"))
        .await
        .expect("open api wallet sqlite");
    let pool = sqlite.get_pool().expect("api wallet db pool");
    ApiWalletDbPool::new(pool)
}

async fn ensure_worker_env() -> &'static WorkerTestEnv {
    WORKER_ENV
        .get_or_init(|| async {
            let (backend_url, recorder) =
                start_mock_backend_server().expect("start mock backend server");
            // Match wallet-api test env setup and disable system proxy resolution for reqwest.
            unsafe {
                std::env::set_var("WALLET_TRANSPORT_NO_PROXY", "1");
            }
            let config = wallet_api::config::Config::new(&format!(
                r#"
app_code: "test"
crypto:
  aes_key: "1234567890abcdef"
  aes_iv: "abcdef1234567890"
backend_api:
  dev_url: "{backend_url}"
  test_url: "{backend_url}"
  prod_url: "{backend_url}"
aggregate_api:
  dev_url: "{backend_url}"
  test_url: "{backend_url}"
  prod_url: "{backend_url}"
oss:
  access_key_id: "id"
  access_key_secret: "secret"
  bucket_name: "bucket"
  endpoint: "oss-endpoint"
"#
            ))
            .expect("parse test config");

            let root = create_test_root_dir();
            let dirs = Dirs::new(root.to_str().expect("utf8 root dir")).expect("create dirs");
            GLOBAL_KEY.set_shared_secret(TEST_PUB_KEY).expect("set shared secret");
            let manager = WalletManager::new_for_test(
                TEST_SN,
                TEST_DEVICE_TYPE,
                config,
                dirs.clone(),
                Arc::new(NoopApiWalletBackend),
            )
            .await
            .expect("create wallet manager");
            wallet_api::infrastructure::system_ready::mark_system_ready();

            WorkerTestEnv { _manager: manager, backend_url, db_dir: dirs.db_dir.clone(), recorder }
        })
        .await
}

async fn current_backend_url() -> Option<String> {
    let app_state = wallet_api::app_state::APP_STATE.read().await;
    app_state.url().backend.clone()
}

fn decrypt_captured_api_backend_body(body: &str) -> serde_json::Value {
    #[derive(serde::Deserialize)]
    struct CapturedApiBackendBody {
        key: String,
        data: String,
    }
    #[derive(serde::Deserialize)]
    struct CapturedApiBackendRequest {
        body: CapturedApiBackendBody,
    }

    let req: CapturedApiBackendRequest =
        serde_json::from_str(body).expect("deserialize captured backend request");
    let key = wallet_utils::base64_to_bytes(&req.body.key).expect("decode encrypted key");
    let data = wallet_utils::base64_to_bytes(&req.body.data).expect("decode encrypted data");
    let plain = GLOBAL_KEY.decrypt(&data, &key).expect("decrypt backend body");
    serde_json::from_slice(&plain).expect("deserialize decrypted payload")
}

#[tokio::test]
async fn collect_blockhash_rebuild_clears_stale_build_facts_and_persists_new_to_addr() {
    let db = TestFundsDb::new().await;
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
    let env = ensure_worker_env().await;
    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no = format!(
        "C_collect_recover_expired_raw_probe_{}",
        UNIQUE_ID.fetch_add(1, Ordering::Relaxed)
    );
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

    let worker = build_shadow_collect_worker(env).await;
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
    let env = ensure_worker_env().await;
    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no =
        format!("C_collect_recover_backfill_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
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

    let worker = build_shadow_collect_worker(env).await;
    worker
        .handle(ShadowCollectCommand::Recover(trade_no.clone()))
        .await
        .expect("recover command should succeed");

    let after = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("reload collect after recover");
    assert_eq!(after.tx_hash.as_deref(), Some(tx_hash));
    assert!(after.transaction_time.is_some());

    let records = ApiCollectRepo::scan_need_tx_exec_receipt_upload(&collect_pool, 100)
        .await
        .expect("scan need tx exec receipt upload");
    assert!(
        records.iter().any(|r| r.trade_no == trade_no),
        "recovered collect with backfilled hash must enter receipt upload scan"
    );
}

fn base_collect_for_receipt() -> ApiCollectEntity {
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

#[tokio::test]
async fn collect_tx_exec_receipt_uses_persisted_to_addr() {
    let req = base_collect_for_receipt();

    let payload = build_collect_tx_exec_receipt_payload(&req, &req.trade_no);
    let payload_json = serde_json::to_value(&payload).expect("serialize payload");

    assert_eq!(payload_json["to"], "persisted-to");
    assert_eq!(payload_json["hash"], "hash");
}

#[tokio::test]
async fn collect_rebuild_then_receipt_upload_uses_rebuilt_to_addr() {
    let db = TestFundsDb::new().await;
    let trade_no = "T_collect_rebuild_then_receipt";

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

    ApiCollectRepo::invalidate_raw_tx_for_rebuild(&db.pool, trade_no, None)
        .await
        .expect("invalidate raw tx for rebuild");

    sqlx::query(
        r#"
        UPDATE api_collect
        SET to_addr = $2,
            tx_hash = $3,
            raw_tx = $4,
            last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(trade_no)
    .bind("new-to")
    .bind("new-hash")
    .bind("{\"rebuilt\":true}")
    .execute(db.pool.as_ref())
    .await
    .expect("persist rebuilt execution facts");

    let rebuilt = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("load rebuilt collect");
    let payload = build_collect_tx_exec_receipt_payload(&rebuilt, trade_no);
    let payload_json = serde_json::to_value(&payload).expect("serialize payload");

    assert_eq!(payload_json["to"], "new-to");
    assert_eq!(payload_json["hash"], "new-hash");
}

#[serial]
#[tokio::test]
async fn collect_side_effect_worker_marks_tx_exec_receipt_uploaded_after_rebuild() {
    let env = ensure_worker_env().await;
    env.recorder.reset();
    let backend_url = current_backend_url().await.expect("backend url set in app state");
    assert_eq!(backend_url, env.backend_url, "worker should use the mock backend URL");

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    let trade_no =
        format!("T_collect_worker_receipt_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));

    ApiCollectRepo::upsert_api_collect(
        &collect_pool,
        "uid",
        "collect",
        "from-worker",
        "old-to",
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
        "USDC",
        &trade_no,
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
    .bind(&trade_no)
    .bind("{\"stale\":true}")
    .bind("old-hash")
    .bind(ApiCollectStatus::SendingTx)
    .execute(collect_pool.as_ref())
    .await
    .expect("set stale build facts");

    ApiCollectRepo::invalidate_raw_tx_for_rebuild(&collect_pool, &trade_no, None)
        .await
        .expect("invalidate raw tx for rebuild");

    sqlx::query(
        r#"
        UPDATE api_collect
        SET to_addr = $2,
            tx_hash = $3,
            raw_tx = $4,
            last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(&trade_no)
    .bind("rebuilt-to")
    .bind("rebuilt-hash")
    .bind("{\"rebuilt\":true}")
    .execute(collect_pool.as_ref())
    .await
    .expect("persist rebuilt execution facts");

    upload_collect_tx_exec_receipt_via_worker(collect_pool.clone(), core_pool, &trade_no)
        .await
        .expect("upload tx exec receipt should succeed");

    let rec = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after worker upload");
    assert!(
        rec.tx_exec_receipt_uploaded_at.is_some(),
        "worker should mark uploaded_at after successful backend upload"
    );

    let payload_json = serde_json::to_value(build_collect_tx_exec_receipt_payload(&rec, &trade_no))
        .expect("serialize rebuilt payload");
    assert_eq!(payload_json["tradeNo"], trade_no);
    assert_eq!(payload_json["to"], "rebuilt-to");
    assert_eq!(payload_json["hash"], "rebuilt-hash");
    assert_eq!(payload_json["status"], "SUCCESS");
}

#[serial]
#[tokio::test]
async fn collect_backend_api_direct_upload_hits_mock_server() {
    let env = ensure_worker_env().await;
    env.recorder.reset();
    let req = ApiCollectEntity {
        trade_no: format!("T_collect_direct_backend_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed)),
        tx_hash: Some("direct-hash".to_string()),
        to_addr: "direct-to".to_string(),
        from_addr: "direct-from".to_string(),
        last_broadcast_at: Some(Utc::now()),
        ..base_collect_for_receipt()
    };

    upload_collect_tx_exec_receipt_via_backend(&req, &req.trade_no)
        .await
        .expect("direct backend upload should succeed");

    let captured = pop_request_with_retry(&env.recorder)
        .await
        .expect("captured backend request for direct upload");
    assert!(
        captured.path.contains("awallet/aw/trans/executeComplete"),
        "unexpected backend path: {}",
        captured.path
    );
    let payload_json = decrypt_captured_api_backend_body(&captured.body);
    assert_eq!(payload_json["tradeNo"], req.trade_no);
    assert_eq!(payload_json["to"], "direct-to");
    assert_eq!(payload_json["hash"], "direct-hash");
    assert_eq!(payload_json["status"], "SUCCESS");
}

#[serial]
#[tokio::test]
async fn collect_notification_retry_on_existing_trade_no() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let uid = format!("uid_collect_notify_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let trade_no = format!("T_collect_notify_retry_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let _wallet_addr =
        seed_wallet(&env.db_dir, &uid, "collect-notify-wallet", ApiWalletType::SubAccount).await;

    let (fail_tx, fail_rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    drop(fail_rx);
    env._manager
        .set_frontend_notify_sender(fail_tx)
        .await
        .expect("install failing frontend sender");

    let first = env
        ._manager
        .api_collect_order(
            "from-collect",
            "to-collect",
            "12.34",
            "digest",
            "sol",
            None,
            "USDC",
            &trade_no,
            2,
            &uid,
        )
        .await;
    assert!(first.is_err(), "frontend notify failure should bubble up");

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after failed notify");
    assert_eq!(persisted.status, ApiCollectStatus::Init);

    let (ok_tx, mut ok_rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    env._manager.set_frontend_notify_sender(ok_tx).await.expect("install working frontend sender");

    env._manager
        .api_collect_order(
            "from-collect",
            "to-collect",
            "12.34",
            "digest",
            "sol",
            None,
            "USDC",
            &trade_no,
            2,
            &uid,
        )
        .await
        .expect("retrying the same collect order should resend frontend notify");

    let notify = tokio::time::timeout(std::time::Duration::from_secs(1), ok_rx.recv())
        .await
        .expect("timed out waiting for collect notify")
        .expect("missing collect notify event");
    let notify_json = serde_json::to_value(&notify).expect("serialize collect notify");
    assert_eq!(notify_json["event"], "COLLECT");
    assert_eq!(notify_json["data"]["uid"], uid);
    assert_eq!(notify_json["data"]["fromAddr"], "from-collect");
    assert_eq!(notify_json["data"]["toAddr"], "to-collect");
    assert_eq!(notify_json["data"]["value"], "12.34");
}

#[serial]
#[tokio::test]
async fn withdraw_notification_retry_on_existing_trade_no() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let uid = format!("uid_withdraw_notify_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let trade_no = format!("T_withdraw_notify_retry_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let _wallet_addr =
        seed_wallet(&env.db_dir, &uid, "withdraw-notify-wallet", ApiWalletType::Withdrawal).await;

    let (fail_tx, fail_rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    drop(fail_rx);
    env._manager
        .set_frontend_notify_sender(fail_tx)
        .await
        .expect("install failing frontend sender");

    let first = env
        ._manager
        .api_withdrawal_order(
            "from-withdraw",
            "to-withdraw",
            "56.78",
            "digest",
            "sol",
            None,
            "USDC",
            &trade_no,
            1,
            &uid,
        )
        .await;
    assert!(first.is_err(), "frontend notify failure should bubble up");

    let tx_pool_ctx = SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    let tx_pool = tx_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let persisted = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
        &tx_pool,
        &trade_no,
        wallet_database::entities::api_trade_type::ApiTradeType::Withdraw,
    )
    .await
    .expect("load withdraw after failed notify");
    assert_eq!(persisted.init_status, ApiWithdrawStatus::AuditPass);
    assert_eq!(persisted.status, ApiWithdrawStatus::InitOrder);

    let (ok_tx, mut ok_rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    env._manager.set_frontend_notify_sender(ok_tx).await.expect("install working frontend sender");

    env._manager
        .api_withdrawal_order(
            "from-withdraw",
            "to-withdraw",
            "56.78",
            "digest",
            "sol",
            None,
            "USDC",
            &trade_no,
            1,
            &uid,
        )
        .await
        .expect("retrying the same withdraw order should resend frontend notify");

    let notify = tokio::time::timeout(std::time::Duration::from_secs(1), ok_rx.recv())
        .await
        .expect("timed out waiting for withdraw notify")
        .expect("missing withdraw notify event");
    let notify_json = serde_json::to_value(&notify).expect("serialize withdraw notify");
    assert_eq!(notify_json["event"], "WITHDRAW");
    assert_eq!(notify_json["data"]["uid"], uid);
    assert_eq!(notify_json["data"]["fromAddr"], "from-withdraw");
    assert_eq!(notify_json["data"]["toAddr"], "to-withdraw");
    assert_eq!(notify_json["data"]["value"], "56.78");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let tx_ack_request_count = loop {
        let requests = env.recorder.snapshot();
        let tx_ack_request_count = requests
            .iter()
            .filter(|req| {
                req.path.contains(
                    wallet_transport_backend::consts::endpoint::api_wallet::TRANS_EVENT_ACK,
                )
            })
            .filter(|req| {
                let payload = decrypt_captured_api_backend_body(&req.body);
                payload["tradeNo"].as_str() == Some(&trade_no)
                    && payload["ackType"].as_str() == Some("TX")
                    && payload["type"].as_str() == Some("WD")
            })
            .count();

        if tx_ack_request_count > 0 || std::time::Instant::now() >= deadline {
            break tx_ack_request_count;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };

    assert_eq!(
        tx_ack_request_count, 1,
        "retrying the same withdraw order should still emit only one TX ack request"
    );
}

#[serial]
#[tokio::test]
async fn withdraw_single_tx_ack_request() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let uid = format!("uid_withdraw_ack_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let trade_no = format!("T_withdraw_ack_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let _wallet_addr =
        seed_wallet(&env.db_dir, &uid, "withdraw-ack-wallet", ApiWalletType::Withdrawal).await;

    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    env._manager
        .set_frontend_notify_sender(notify_tx)
        .await
        .expect("install working frontend sender");

    env._manager
        .api_withdrawal_order(
            "from-withdraw",
            "to-withdraw",
            "56.78",
            "digest",
            "sol",
            None,
            "USDC",
            &trade_no,
            1,
            &uid,
        )
        .await
        .expect("withdraw order should succeed");

    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), notify_rx.recv())
        .await
        .expect("timed out waiting for withdraw notify")
        .expect("missing withdraw notify event");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let tx_ack_request_count = loop {
        let requests = env.recorder.snapshot();
        let tx_ack_request_count = requests
            .iter()
            .filter(|req| {
                req.path.contains(
                    wallet_transport_backend::consts::endpoint::api_wallet::TRANS_EVENT_ACK,
                )
            })
            .filter(|req| {
                let payload = decrypt_captured_api_backend_body(&req.body);
                payload["tradeNo"].as_str() == Some(&trade_no)
                    && payload["ackType"].as_str() == Some("TX")
                    && payload["type"].as_str() == Some("WD")
            })
            .count();

        if tx_ack_request_count > 0 || std::time::Instant::now() >= deadline {
            break tx_ack_request_count;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };

    assert_eq!(tx_ack_request_count, 1, "withdraw order should emit exactly one TX ack request");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let requests = env.recorder.snapshot();
    let tx_ack_request_count = requests
        .iter()
        .filter(|req| {
            req.path
                .contains(wallet_transport_backend::consts::endpoint::api_wallet::TRANS_EVENT_ACK)
        })
        .filter(|req| {
            let payload = decrypt_captured_api_backend_body(&req.body);
            payload["tradeNo"].as_str() == Some(&trade_no)
                && payload["ackType"].as_str() == Some("TX")
                && payload["type"].as_str() == Some("WD")
        })
        .count();
    assert_eq!(tx_ack_request_count, 1, "withdraw order should not emit a second TX ack request");
}

#[serial]
#[tokio::test]
async fn collect_sol_native_fee_check_fails_on_uninitialized_recipient() {
    let env = ensure_worker_env().await;
    let _guard = install_collect_test_adapter(true, 27_309_206);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no = format!("T_collect_sol_rent_fail_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let req = seed_collect_order(
        &collect_pool,
        &trade_no,
        "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
    )
    .await;

    let worker = build_shadow_collect_worker(env).await;
    let err = shadow_collect_check_fee(&worker, &req)
        .await
        .expect_err("uninitialized SOL recipient should fail fee check");
    let msg = err.to_string();
    assert!(msg.contains("recipient account is not initialized"));
    assert!(msg.contains("rent-exempt minimum"));

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after failure");
    assert_eq!(persisted.status, ApiCollectStatus::Init);
}

#[serial]
#[tokio::test]
async fn collect_sol_native_fee_check_allows_initialized_recipient() {
    let env = ensure_worker_env().await;
    let _guard = install_collect_test_adapter(false, 27_309_206);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no = format!("T_collect_sol_rent_ok_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let req = seed_collect_order(
        &collect_pool,
        &trade_no,
        "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
    )
    .await;

    let worker = build_shadow_collect_worker(env).await;
    let pass = shadow_collect_check_fee(&worker, &req)
        .await
        .expect("initialized SOL recipient should pass fee check");
    assert!(pass);

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after success");
    assert_eq!(persisted.status, ApiCollectStatus::Init);
}

#[serial]
#[tokio::test]
async fn collect_eth_native_fee_check_with_partial_oracle_fallback() {
    let env = ensure_worker_env().await;
    let _guard =
        install_collect_eth_test_adapter(U256::from(100_000_000_000_000_000u128), 0.00000368);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no =
        format!("T_collect_eth_partial_oracle_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let req = seed_eth_collect_order(
        &collect_pool,
        &trade_no,
        "0x477000C778C66FaAA36596Fb846Ce34C89bc652D",
        "0xFCa230313618af2a33fa00455D8A5d1466C91332",
        "0.000015",
    )
    .await;

    let worker = build_eth_shadow_collect_worker(env).await;
    let pass = shadow_collect_check_fee(&worker, &req)
        .await
        .expect("ETH collect fee check should succeed with partial oracle fallback");
    assert!(pass);

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after success");
    assert_eq!(persisted.status, ApiCollectStatus::Init);
}

#[serial]
#[tokio::test]
async fn collect_eth_native_fee_check_fails_on_insufficient_balance() {
    let env = ensure_worker_env().await;
    let _guard = install_collect_eth_test_adapter(U256::from(10_000_000_000_000u128), 0.00000368);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no =
        format!("T_collect_eth_insufficient_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let req = seed_eth_collect_order(
        &collect_pool,
        &trade_no,
        "0x477000C778C66FaAA36596Fb846Ce34C89bc652D",
        "0xFCa230313618af2a33fa00455D8A5d1466C91332",
        "0.000015",
    )
    .await;

    let worker = build_eth_shadow_collect_worker(env).await;
    let pass = shadow_collect_check_fee(&worker, &req)
        .await
        .expect("ETH collect fee check should return a boolean result");
    assert!(!pass, "insufficient ETH balance should fail the fee check");

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after failure");
    assert_eq!(persisted.status, ApiCollectStatus::Init);
}

#[serial]
#[tokio::test]
async fn collect_build_fee_estimation_shortage_reopens_fee_cycle() {
    let env = ensure_worker_env().await;
    let _guard = install_collect_test_adapter_fee_shortage(false, 0);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no = format!("T_collect_fee_shortage_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let req = seed_collect_order(
        &collect_pool,
        &trade_no,
        "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
    )
    .await;

    let worker = build_shadow_collect_worker(env).await;
    let pass = shadow_collect_check_fee(&worker, &req)
        .await
        .expect("fee estimate shortage should be downgraded to a boolean result");
    assert!(!pass, "fee estimate shortage must reopen the service-fee cycle instead of erroring");

    let affected = worker
        .invalidate_build_attempt_after_fee_check_failure(&req)
        .await
        .expect("fee shortage should reopen the fee cycle");
    assert_eq!(affected, 1);

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after fee shortage reopen");
    assert_eq!(persisted.need_service_fee, Some(true));
    assert!(persisted.service_fee_uploaded_at.is_none());
    assert!(persisted.raw_tx.is_none());
    assert!(persisted.tx_hash.is_none());
}

#[serial]
#[tokio::test]
async fn collect_service_fee_upload_bypasses_local_sol_fee_gate() {
    let env = ensure_worker_env().await;
    env.recorder.reset();
    let _guard = install_collect_test_adapter_fee_shortage(false, 0);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    ensure_sol_main_coin(&core_pool).await;

    let now = Utc::now();
    let usdc_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
    ApiCoinRepo::upsert_multi_coin(
        &core_pool,
        vec![ApiCoinData::new(
            Some("Solana".to_string()),
            "USDC",
            "sol",
            AssetTokenKey::Contract(usdc_mint.to_string()),
            Some("0".to_string()),
            None,
            6,
            1,
            1,
            1,
            now,
            Some(now),
        )],
    )
    .await
    .expect("seed sol usdc coin");

    let trade_no =
        format!("T_collect_service_fee_upload_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let collect_uid = format!("collect-uid-{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let withdrawal_uid = format!("withdraw-uid-{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let from_addr = "DLcQZyqoL7ghnENR4mboeuivCNAKXBWJ8RKQA9aK3ZW8";
    let to_addr = "72vgdLcQgdudUiGXudHNPhgCPNPCdxj2ijAGuXTQ5ppB";

    let withdrawal_wallet =
        upsert_wallet(&env.db_dir, "sn-collect", &withdrawal_uid, ApiWalletType::Withdrawal, None)
            .await;
    let subaccount_wallet = upsert_wallet(
        &env.db_dir,
        "sn-collect",
        &collect_uid,
        ApiWalletType::SubAccount,
        Some(&withdrawal_wallet),
    )
    .await;

    let account = CreateApiAccountVo::new(
        1,
        from_addr,
        "pubkey",
        &subaccount_wallet,
        &collect_uid,
        "m/44'/501'/0'/0/0",
        0,
        "sol",
        "account",
        ApiWalletType::SubAccount,
    )
    .with_is_init(true);
    ApiAccountRepo::upsert_account_multi(&core_pool, vec![account])
        .await
        .expect("seed collect account");

    let withdraw_strategy = ApiWithdrawStrategyEntity {
        id: 0,
        uid: withdrawal_uid.to_string(),
        threshold: 50,
        created_at: Utc::now(),
        updated_at: None,
    };
    ApiWithdrawStrategyRepo::upsert(&core_pool, withdraw_strategy)
        .await
        .expect("seed withdraw strategy");
    let withdraw_strategy_id = ApiWithdrawStrategyRepo::get_by_uid(&core_pool, &withdrawal_uid)
        .await
        .expect("load withdraw strategy")
        .expect("withdraw strategy exists")
        .id;
    ApiWithdrawStrategyChainConfigRepo::upsert(
        &core_pool,
        ApiWithdrawStrategyChainConfigEntity {
            id: 0,
            strategy_id: withdraw_strategy_id,
            chain_code: "sol".to_string(),
            chain_address_type: None,
            normal_idx: Some(0),
            normal_address: to_addr.to_string(),
            risk_idx: Some(1),
            risk_address: to_addr.to_string(),
            created_at: Utc::now(),
            updated_at: None,
        },
    )
    .await
    .expect("seed withdraw strategy chain config");

    ApiCollectRepo::upsert_api_collect(
        &collect_pool,
        &collect_uid,
        "collect",
        from_addr,
        to_addr,
        "1.1",
        "digest",
        "sol",
        Some(usdc_mint.to_string()),
        "USDC",
        &trade_no,
        2,
        ApiCollectStatus::InsufficientBalance,
        1,
    )
    .await
    .expect("seed collect row");

    sqlx::query(
        r#"
        UPDATE api_collect
        SET need_service_fee = true,
            ever_needed_service_fee = true,
            service_fee_uploaded_at = NULL,
            service_fee_order_received_at = NULL,
            transaction_fee = '',
            status = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = ?
        "#,
    )
    .bind(ApiCollectStatus::InsufficientBalance)
    .bind(&trade_no)
    .execute(collect_pool.as_ref())
    .await
    .expect("seed fee-wait row");

    upload_collect_service_fee_via_worker(collect_pool.clone(), core_pool, &trade_no)
        .await
        .expect("service fee upload should bypass local balance gate");

    let requests = env.recorder.snapshot();
    let request = requests
        .iter()
        .find(|req| {
            req.path.contains(
                wallet_transport_backend::consts::endpoint::api_wallet::TRANS_SERVICE_FEE_TRANS,
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "service fee upload must call the fee-trans endpoint, captured paths: {:?}",
                requests.iter().map(|req| req.path.clone()).collect::<Vec<_>>()
            )
        });

    let payload = decrypt_captured_api_backend_body(&request.body);
    assert_eq!(payload["tradeNo"].as_str(), Some(trade_no.as_str()));
    assert_eq!(payload["from"].as_str(), Some(to_addr));
    assert_eq!(payload["to"].as_str(), Some(from_addr));
    assert_eq!(payload["tokenCode"].as_str(), Some("SOL"));
    assert_eq!(payload["contractAddress"].as_str(), Some(""));
    assert!(
        payload["amount"].as_f64().unwrap_or_default() > 0.0,
        "service fee upload must carry a non-zero fee amount"
    );
}

#[serial]
#[tokio::test]
async fn collect_build_fee_failure_reopens_fee_cycle_on_first_insufficient_balance() {
    let env = ensure_worker_env().await;
    let _guard = install_collect_test_adapter(false, 0);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no =
        format!("T_collect_fee_reopen_initial_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let req = seed_collect_order(
        &collect_pool,
        &trade_no,
        "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
    )
    .await;

    let worker = build_shadow_collect_worker(env).await;
    let pass = shadow_collect_check_fee(&worker, &req)
        .await
        .expect("fee check should return a boolean result");
    assert!(!pass, "low balance should still fail the build fee check on the first attempt");

    let affected = worker
        .invalidate_build_attempt_after_fee_check_failure(&req)
        .await
        .expect("first insufficient balance should reopen fee cycle");
    assert_eq!(affected, 1);

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after reopen");
    assert_eq!(persisted.need_service_fee, Some(true));
    assert!(persisted.service_fee_uploaded_at.is_none());
    assert!(persisted.raw_tx.is_none());
    assert!(persisted.tx_hash.is_none());
}

#[serial]
#[tokio::test]
async fn collect_build_fee_failure_preserves_completed_fee_cycle_facts() {
    let env = ensure_worker_env().await;
    let _guard = install_collect_test_adapter(false, 0);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no =
        format!("T_collect_fee_reopen_rebuild_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));
    let mut req = seed_collect_order(
        &collect_pool,
        &trade_no,
        "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
    )
    .await;

    sqlx::query(
        r#"
        UPDATE api_collect
        SET service_fee_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            tx_fee_res_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            need_service_fee = false,
            ever_needed_service_fee = true,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = ?
        "#,
    )
    .bind(&trade_no)
    .execute(collect_pool.as_ref())
    .await
    .expect("seed fee cycle facts");

    req = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("reload collect with fee cycle facts");

    let worker = build_shadow_collect_worker(env).await;
    let pass = shadow_collect_check_fee(&worker, &req)
        .await
        .expect("fee check should return a boolean result");
    assert!(
        !pass,
        "low balance should still fail the build fee check when fee facts already exist"
    );

    let affected = worker
        .invalidate_build_attempt_after_fee_check_failure(&req)
        .await
        .expect("completed fee cycle should preserve fee facts during rebuild");
    assert_eq!(affected, 1);

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after rebuild-only invalidation");
    assert_eq!(persisted.need_service_fee, Some(false));
    assert!(persisted.service_fee_uploaded_at.is_some());
    assert!(persisted.tx_fee_res_ack_sent_at.is_some());
    assert!(persisted.raw_tx.is_none());
    assert!(persisted.tx_hash.is_none());
}

#[tokio::test]
async fn collect_scanner_skips_stale_fee_cycle_rows() {
    let db = TestFundsDb::new().await;
    let trade_no = format!("T_collect_scanner_stale_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));

    ApiCollectRepo::upsert_api_collect(
        &db.pool,
        "uid",
        "collect",
        "from-scan",
        "to-scan",
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
        "USDC",
        &trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

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
    .bind(&trade_no)
    .execute(db.pool.as_ref())
    .await
    .expect("seed stale fee-cycle row");

    let labels = scan_collect_intent_labels_once(db.pool.clone())
        .await
        .expect("scanner round should succeed");

    assert!(labels.is_empty(), "stale fee-cycle row must not re-enter build / fee-ack scanning");

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, &trade_no)
        .await
        .expect("load collect after scanner round");
    assert_eq!(persisted.need_service_fee, Some(true));
    assert!(persisted.service_fee_uploaded_at.is_some());
    assert!(persisted.raw_tx.is_none());
    assert!(persisted.tx_hash.is_none());
}

#[tokio::test]
async fn collect_scanner_emits_upload_service_fee_when_need_service_fee_is_true() {
    let db = TestFundsDb::new().await;
    let trade_no = format!("T_collect_wait_fee_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));

    ApiCollectRepo::upsert_api_collect(
        &db.pool,
        "uid",
        "collect",
        "from-wait",
        "to-wait",
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
        "USDC",
        &trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

    sqlx::query(
        r#"
        UPDATE api_collect
        SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            need_service_fee = true,
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
    .bind(&trade_no)
    .execute(db.pool.as_ref())
    .await
    .expect("seed waiting fee-cycle row");

    let labels = scan_collect_intent_labels_once(db.pool.clone())
        .await
        .expect("scanner round should succeed");

    assert!(
        labels.iter().any(|label| label == "UploadServiceFee"),
        "active fee-wait row must emit UploadServiceFee immediately"
    );
    assert!(
        labels.iter().all(|label| label != "BuildTx"),
        "fee upload should not bypass fee-cycle gating into build"
    );

    let persisted_after = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, &trade_no)
        .await
        .expect("load collect after scanner round");
    assert_eq!(persisted_after.need_service_fee, Some(true));
    assert!(persisted_after.service_fee_order_received_at.is_none());
    assert!(persisted_after.service_fee_uploaded_at.is_none());
    assert!(persisted_after.raw_tx.is_none());
    assert!(persisted_after.tx_hash.is_none());
}

#[tokio::test]
async fn collect_scanner_recovers_broadcast_visible_pending_result() {
    let db = TestFundsDb::new().await;
    let trade_no = format!("T_collect_recover_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));

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

#[tokio::test]
async fn collect_scanner_emits_tx_fee_res_ack_before_build_after_fee_result() {
    let db = TestFundsDb::new().await;
    let trade_no = format!("T_collect_fee_ack_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));

    seed_collect_order(&db.pool, &trade_no, "to-fee-ack").await;

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
    .bind(&trade_no)
    .execute(db.pool.as_ref())
    .await
    .expect("seed completed fee-cycle row");

    let labels = scan_collect_intent_labels_once(db.pool.clone())
        .await
        .expect("scanner round should succeed");

    assert!(
        labels.iter().any(|label| label == "SendTxFeeResAck"),
        "fee-result row must emit TxFeeResAck"
    );
    assert!(
        labels.iter().all(|label| label != "BuildTx"),
        "fee-result ACK must be sent before build is allowed again"
    );

    let persisted_after = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, &trade_no)
        .await
        .expect("load collect after scanner round");
    assert_eq!(persisted_after.need_service_fee, Some(false));
    assert!(persisted_after.tx_fee_res_ack_sent_at.is_none());
    assert!(persisted_after.raw_tx.is_none());
}

#[serial]
#[tokio::test]
async fn collect_scanner_dispatcher_uploads_rebuilt_tx_exec_receipt() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    let trade_no = format!("T_collect_scan_dispatch_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));

    ApiCollectRepo::upsert_api_collect(
        &collect_pool,
        "uid",
        "collect",
        "from-scan",
        "rebuilt-to",
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
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
        SET tx_hash = $2,
            raw_tx = $3,
            last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            tx_exec_receipt_uploaded_at = NULL,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(&trade_no)
    .bind("scan-hash")
    .bind("{\"rebuilt\":true}")
    .execute(collect_pool.as_ref())
    .await
    .expect("persist scan facts");

    let dispatched_trade_no =
        scan_and_dispatch_collect_tx_exec_receipt_once(collect_pool.clone(), core_pool)
            .await
            .expect("scanner-dispatcher flow should succeed");
    assert_eq!(dispatched_trade_no.as_deref(), Some(trade_no.as_str()));

    let rec = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after scanner-dispatcher");
    assert!(
        rec.tx_exec_receipt_uploaded_at.is_some(),
        "scanner-dispatcher should mark uploaded_at"
    );

    let payload_json = serde_json::to_value(build_collect_tx_exec_receipt_payload(&rec, &trade_no))
        .expect("serialize scanner-dispatch payload");
    assert_eq!(payload_json["tradeNo"], trade_no);
    assert_eq!(payload_json["to"], "rebuilt-to");
    assert_eq!(payload_json["hash"], "scan-hash");
    assert_eq!(payload_json["status"], "SUCCESS");
}
