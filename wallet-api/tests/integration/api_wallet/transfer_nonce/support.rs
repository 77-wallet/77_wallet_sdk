use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use alloy::primitives::U256;
use chrono::Utc;
use tokio::{sync::Notify, task::JoinHandle};
use wallet_api::{
    error::service::ServiceError,
    request::api_wallet::{trans::ApiBaseTransferReq, transfer::ApiTransferExReq},
    testkit::adapter_factory::{
        clear_test_transaction_adapter_override, set_test_transaction_adapter_override,
    },
};
use wallet_chain_interact::{Error as ChainError, QueryTransactionResult};
use wallet_database::{
    ApiTransactionDbPool, SqliteContext,
    entities::{
        api_account::CreateApiAccountVo, api_coin::ApiCoinData, api_wallet::ApiWalletType,
        asset_token_key::AssetTokenKey,
    },
    repositories::api_wallet::{account::ApiAccountRepo, coin::ApiCoinRepo, nonce::ApiNonceRepo},
};

use crate::harness::{
    self, SMOKE_WALLET_PASSWORD, ensure_env, next_tag, reset_fake, upsert_wallet,
};

const BNB_TO_ADDR: &str = "0x998522f928A37837Fa8d6743713170243b95f98a";

pub(super) struct TransferNonceScenario {
    env: &'static harness::TestEnv,
    from_addr: String,
    adapter: RecordingEthAdapter,
    _guard: Option<AdapterGuard>,
}

impl TransferNonceScenario {
    pub(super) async fn new() -> Self {
        Self {
            env: ensure_env().await,
            from_addr: String::new(),
            adapter: RecordingEthAdapter::new(),
            _guard: None,
        }
    }

    pub(super) async fn given_bnb_transfer_fixture(&mut self) -> anyhow::Result<()> {
        self.from_addr = ensure_bnb_transfer_fixture(self.env).await?;
        Ok(())
    }

    pub(super) fn given_first_transfer_blocks(&self) {
        self.adapter.block_first_transfer();
    }

    pub(super) fn given_transfer_fails(&self) {
        self.adapter.fail_on_transfer();
    }

    pub(super) fn given_fake_chain_adapter(&mut self) {
        self._guard = Some(install_adapter(&self.adapter));
    }

    pub(super) async fn given_wallet_password_cached(&self) {
        let _ = self.env.manager.set_passwd_cache(SMOKE_WALLET_PASSWORD).await;
    }

    pub(super) fn when_transfer_starts(&self) -> JoinHandle<Result<String, ServiceError>> {
        let wallet_manager = self.env.manager.clone();
        let req = make_transfer_req(&self.from_addr, BNB_TO_ADDR);

        tokio::spawn(async move {
            let resp =
                wallet_manager.api_transfer_with_preloaded_private_key(req, "00".into()).await?;
            Ok(resp.tx_hash)
        })
    }

    pub(super) async fn when_transfer_fails(&self) -> ServiceError {
        let req = make_transfer_req(&self.from_addr, BNB_TO_ADDR);
        self.env
            .manager
            .api_transfer_with_preloaded_private_key(req, "00".into())
            .await
            .expect_err("transfer should fail")
    }

    pub(super) async fn then_first_transfer_has_entered(&self) {
        self.adapter.wait_for_first_entry().await;
    }

    pub(super) async fn then_only_first_nonce_is_recorded_while_second_waits(&self) {
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(self.adapter.recorded_nonces(), vec![1], "second transfer should stay blocked");
    }

    pub(super) fn when_first_transfer_is_released(&self) {
        self.adapter.release_first();
    }

    pub(super) async fn then_serial_transfer_results_are(
        &self,
        first: JoinHandle<Result<String, ServiceError>>,
        second: JoinHandle<Result<String, ServiceError>>,
    ) -> anyhow::Result<()> {
        let first_result = first.await.expect("first join")?;
        let second_result = second.await.expect("second join")?;

        assert_eq!(first_result, format!("0x{:064x}", 1u64));
        assert_eq!(second_result, format!("0x{:064x}", 2u64));
        assert_eq!(self.adapter.recorded_nonces(), vec![1, 2]);

        Ok(())
    }

    pub(super) async fn then_failure_keeps_reserved_nonce(
        &self,
        err: ServiceError,
    ) -> anyhow::Result<()> {
        assert!(err.to_string().contains("simulated transfer failure"), "unexpected error: {err}");

        let tx_pool = open_api_transaction_pool(&self.env.db_dir).await;
        let floor = ApiNonceRepo::get_api_nonce(&tx_pool, &self.from_addr, "bnb").await?;
        assert_eq!(floor, 1, "nonce floor should stay advanced after failure");
        assert_eq!(self.adapter.recorded_nonces(), vec![1]);

        Ok(())
    }
}

#[derive(Default)]
struct RecordingAdapterState {
    recorded_nonces: Mutex<Vec<u64>>,
    call_count: AtomicUsize,
    first_entered: AtomicBool,
    fail_transfer: AtomicBool,
    block_first: AtomicBool,
    release_first: Notify,
    entered_notify: Notify,
}

#[derive(Clone)]
struct RecordingEthAdapter {
    state: Arc<RecordingAdapterState>,
}

impl RecordingEthAdapter {
    fn new() -> Self {
        Self { state: Arc::new(RecordingAdapterState::default()) }
    }

    fn fail_on_transfer(&self) {
        self.state.fail_transfer.store(true, Ordering::SeqCst);
    }

    fn block_first_transfer(&self) {
        self.state.block_first.store(true, Ordering::SeqCst);
    }

    fn recorded_nonces(&self) -> Vec<u64> {
        self.state.recorded_nonces.lock().expect("nonce lock").clone()
    }

    async fn wait_for_first_entry(&self) {
        loop {
            if self.state.first_entered.load(Ordering::SeqCst) {
                return;
            }
            self.state.entered_notify.notified().await;
        }
    }

    fn release_first(&self) {
        self.state.release_first.notify_waiters();
    }
}

#[async_trait::async_trait]
impl wallet_api::domain::api_wallet::Tx for RecordingEthAdapter {
    async fn account_resource(
        &self,
        _owner_address: &str,
    ) -> Result<wallet_chain_interact::tron::protocol::account::AccountResourceDetail, ServiceError>
    {
        unimplemented!()
    }

    async fn balance_token_key(
        &self,
        _addr: &str,
        _token: AssetTokenKey,
    ) -> Result<U256, ChainError> {
        Ok(U256::ZERO)
    }

    async fn nonce(&self, _addr: &str) -> Result<u64, ServiceError> {
        Ok(0)
    }

    async fn block_num(&self) -> Result<u64, ChainError> {
        Ok(0)
    }

    async fn query_tx_res(
        &self,
        _hash: &str,
    ) -> Result<Option<QueryTransactionResult>, ChainError> {
        Ok(None)
    }

    async fn token_symbol(&self, _token: &str) -> Result<String, ChainError> {
        Ok("BNB".to_string())
    }

    async fn token_name(&self, _token: &str) -> Result<String, ChainError> {
        Ok("BNB Smart Chain".to_string())
    }

    async fn decimals(&self, _token: &str) -> Result<u8, ChainError> {
        Ok(18)
    }

    async fn black_address(&self, _token: &str, _owner: &str) -> Result<bool, ServiceError> {
        Ok(false)
    }

    async fn transfer(
        &self,
        params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<wallet_api::domain::chain::TransferResp, ServiceError> {
        let call_idx = self.state.call_count.fetch_add(1, Ordering::SeqCst);
        {
            let mut nonces = self.state.recorded_nonces.lock().expect("nonce lock");
            nonces.push(params.nonce);
        }

        if call_idx == 0 && self.state.block_first.load(Ordering::SeqCst) {
            self.state.first_entered.store(true, Ordering::SeqCst);
            self.state.entered_notify.notify_waiters();
            self.state.release_first.notified().await;
        }

        if self.state.fail_transfer.load(Ordering::SeqCst) {
            return Err(ServiceError::Parameter("simulated transfer failure".to_string()));
        }

        Ok(wallet_api::domain::chain::TransferResp::new(
            format!("0x{:064x}", params.nonce),
            "0".to_string(),
        ))
    }

    async fn estimate_fee(
        &self,
        _req: ApiBaseTransferReq,
        _main_symbol: &str,
    ) -> Result<String, ServiceError> {
        Ok("0".to_string())
    }

    async fn build_transfer_raw(
        &self,
        _params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<(String, wallet_api::domain::api_wallet::RawTx, String), ServiceError> {
        unimplemented!()
    }

    async fn broadcast_transfer(
        &self,
        _raw: wallet_api::domain::api_wallet::RawTx,
    ) -> Result<wallet_api::domain::chain::TransferResp, ServiceError> {
        unimplemented!()
    }
}

struct AdapterGuard {
    chain_code: String,
}

impl Drop for AdapterGuard {
    fn drop(&mut self) {
        clear_test_transaction_adapter_override(&self.chain_code);
    }
}

async fn open_api_transaction_pool(db_dir: &Path) -> ApiTransactionDbPool {
    let sqlite = SqliteContext::new(&db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    sqlite.into_transaction_db_pool().expect("api transaction pool")
}

async fn ensure_bnb_transfer_fixture(env: &harness::TestEnv) -> anyhow::Result<String> {
    reset_fake(env);

    let api_pool = harness::open_api_wallet_pool(&env.db_dir).await;

    if ApiCoinRepo::coin_by_chain_token_key_opt("bnb", AssetTokenKey::Native, &api_pool)
        .await?
        .is_none()
    {
        let now = Utc::now();
        ApiCoinRepo::upsert_multi_coin(
            &api_pool,
            vec![ApiCoinData::new(
                Some("BNB Smart Chain".to_string()),
                "BNB",
                "bnb",
                AssetTokenKey::Native,
                Some("0".to_string()),
                None,
                18,
                1,
                1,
                1,
                now,
                Some(now),
            )],
        )
        .await?;
    }

    let wallet_uid = next_tag("wallet-uid");
    let wallet_address =
        upsert_wallet(&env.db_dir, &env.sn, &wallet_uid, ApiWalletType::SubAccount, None).await;

    let account = CreateApiAccountVo::new(
        1,
        &wallet_address,
        "pubkey",
        &wallet_address,
        &wallet_uid,
        "m/44'/60'/0'/0/0",
        0,
        "bnb",
        "account",
        ApiWalletType::SubAccount,
    )
    .with_is_init(true);
    ApiAccountRepo::upsert_account_multi(&api_pool, vec![account]).await?;

    let tx_pool = open_api_transaction_pool(&env.db_dir).await;
    ApiNonceRepo::set_nonce_floor(&tx_pool, &wallet_address, "bnb", 0).await?;

    Ok(wallet_address)
}

fn install_adapter(adapter: &RecordingEthAdapter) -> AdapterGuard {
    let chain_code = "bnb".to_string();
    let adapter: Arc<dyn wallet_api::domain::api_wallet::Tx + Send + Sync> =
        Arc::new(adapter.clone());
    set_test_transaction_adapter_override(&chain_code, adapter);
    AdapterGuard { chain_code }
}

fn make_transfer_req(from: &str, to: &str) -> ApiTransferExReq {
    let mut base = ApiBaseTransferReq::new(from, to, "0.0000001", "bnb");
    base.with_token(None, 18, "BNB");
    ApiTransferExReq {
        base,
        password: SMOKE_WALLET_PASSWORD.to_string(),
        fee_setting: "".to_string(),
        signer: None,
    }
}
