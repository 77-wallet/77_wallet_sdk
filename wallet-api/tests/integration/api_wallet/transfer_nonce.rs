use crate::harness::{
    self, SMOKE_WALLET_PASSWORD, ensure_env, next_tag, reset_fake, upsert_wallet,
};
use alloy::primitives::U256;
use chrono::Utc;
use serial_test::serial;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use wallet_api::{
    request::api_wallet::{trans::ApiBaseTransferReq, transfer::ApiTransferExReq},
    test_support::adapter_factory::{
        clear_test_transaction_adapter_override, set_test_transaction_adapter_override,
    },
};
use wallet_chain_interact::{Error as ChainError, QueryTransactionResult};
use wallet_database::{
    SqliteContext,
    entities::{
        api_account::CreateApiAccountVo, api_coin::ApiCoinData, api_wallet::ApiWalletType,
        asset_token_key::AssetTokenKey,
    },
    repositories::api_wallet::{account::ApiAccountRepo, coin::ApiCoinRepo, nonce::ApiNonceRepo},
};

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
    ) -> Result<
        wallet_chain_interact::tron::protocol::account::AccountResourceDetail,
        wallet_api::error::service::ServiceError,
    > {
        unimplemented!()
    }

    async fn balance_token_key(
        &self,
        _addr: &str,
        _token: AssetTokenKey,
    ) -> Result<U256, ChainError> {
        Ok(U256::ZERO)
    }

    async fn nonce(&self, _addr: &str) -> Result<u64, wallet_api::error::service::ServiceError> {
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

    async fn black_address(
        &self,
        _token: &str,
        _owner: &str,
    ) -> Result<bool, wallet_api::error::service::ServiceError> {
        Ok(false)
    }

    async fn transfer(
        &self,
        params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<wallet_api::domain::chain::TransferResp, wallet_api::error::service::ServiceError>
    {
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
            return Err(wallet_api::error::service::ServiceError::Parameter(
                "simulated transfer failure".to_string(),
            ));
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
    ) -> Result<String, wallet_api::error::service::ServiceError> {
        Ok("0".to_string())
    }

    async fn build_transfer_raw(
        &self,
        _params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<
        (String, wallet_api::domain::api_wallet::RawTx, String),
        wallet_api::error::service::ServiceError,
    > {
        unimplemented!()
    }

    async fn broadcast_transfer(
        &self,
        _raw: wallet_api::domain::api_wallet::RawTx,
    ) -> Result<wallet_api::domain::chain::TransferResp, wallet_api::error::service::ServiceError>
    {
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

async fn open_api_transaction_pool(
    db_dir: &std::path::Path,
) -> wallet_database::ApiTransactionDbPool {
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

#[tokio::test]
#[serial]
async fn api_wallet_transfer_nonce_lock_keeps_same_address_requests_serial() -> anyhow::Result<()> {
    let env = ensure_env().await;
    let from_addr = ensure_bnb_transfer_fixture(env).await?;
    let adapter = RecordingEthAdapter::new();
    adapter.block_first_transfer();
    let _guard = install_adapter(&adapter);
    let wallet_manager = env.manager.clone();
    let _ = wallet_manager.set_passwd_cache(SMOKE_WALLET_PASSWORD).await;

    let req = make_transfer_req(&from_addr, "0x998522f928A37837Fa8d6743713170243b95f98a");

    let first = tokio::spawn({
        let wallet_manager = wallet_manager.clone();
        let req = req.clone();
        async move {
            let resp =
                wallet_manager.api_transfer_with_preloaded_private_key(req, "00".into()).await?;
            Ok::<_, wallet_api::error::service::ServiceError>(resp.tx_hash)
        }
    });

    adapter.wait_for_first_entry().await;

    let second = tokio::spawn({
        let wallet_manager = wallet_manager.clone();
        let req = req.clone();
        async move {
            let resp =
                wallet_manager.api_transfer_with_preloaded_private_key(req, "00".into()).await?;
            Ok::<_, wallet_api::error::service::ServiceError>(resp.tx_hash)
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(adapter.recorded_nonces(), vec![1], "second transfer should stay blocked");

    adapter.release_first();

    let first_result = first.await.expect("first join")?;
    let second_result = second.await.expect("second join")?;
    assert_eq!(first_result, format!("0x{:064x}", 1u64));
    assert_eq!(second_result, format!("0x{:064x}", 2u64));
    assert_eq!(adapter.recorded_nonces(), vec![1, 2]);

    Ok(())
}

#[tokio::test]
#[serial]
async fn api_wallet_transfer_nonce_failure_keeps_reserved_nonce() -> anyhow::Result<()> {
    let env = ensure_env().await;
    let from_addr = ensure_bnb_transfer_fixture(env).await?;
    let adapter = RecordingEthAdapter::new();
    adapter.fail_on_transfer();
    let _guard = install_adapter(&adapter);
    let wallet_manager = env.manager.clone();
    let _ = wallet_manager.set_passwd_cache(SMOKE_WALLET_PASSWORD).await;

    let req = make_transfer_req(&from_addr, "0x998522f928A37837Fa8d6743713170243b95f98a");
    let err = wallet_manager
        .api_transfer_with_preloaded_private_key(req, "00".into())
        .await
        .expect_err("transfer should fail");
    assert!(err.to_string().contains("simulated transfer failure"), "unexpected error: {err}");

    let tx_pool = open_api_transaction_pool(&env.db_dir).await;
    let floor = ApiNonceRepo::get_api_nonce(&tx_pool, &from_addr, "bnb").await?;
    assert_eq!(floor, 1, "nonce floor should stay advanced after failure");
    assert_eq!(adapter.recorded_nonces(), vec![1]);

    Ok(())
}
