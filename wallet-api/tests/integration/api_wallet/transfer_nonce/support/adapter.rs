use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use alloy::primitives::U256;
use tokio::sync::Notify;
use wallet_api::{
    error::service::ServiceError,
    request::api_wallet::trans::ApiBaseTransferReq,
    testkit::adapter_factory::{
        clear_test_transaction_adapter_override, set_test_transaction_adapter_override,
    },
};
use wallet_chain_interact::{Error as ChainError, QueryTransactionResult};
use wallet_database::entities::asset_token_key::AssetTokenKey;

#[derive(Default)]
struct RecordingAdapterState {
    recorded_nonces: Mutex<Vec<u64>>,
    recorded_raw: Mutex<Vec<Vec<u8>>>,
    build_calls: AtomicUsize,
    canonical_hash: AtomicBool,
    visibility_error: AtomicBool,

    receipt_behavior: Mutex<Option<(String, i8)>>,
    receipt_queries: AtomicUsize,

    nonce_response: Mutex<Option<u64>>,
    nonce_queries: AtomicUsize,
    call_count: AtomicUsize,
    first_entered: AtomicBool,
    fail_transfer: AtomicBool,
    timeout_transfer: AtomicBool,
    invisible_transfer: AtomicBool,
    mismatch_hash: AtomicBool,
    block_first: AtomicBool,
    release_first: Notify,
    entered_notify: Notify,
}

#[derive(Clone)]
pub(crate) struct RecordingEthAdapter {
    state: Arc<RecordingAdapterState>,
}

impl RecordingEthAdapter {
    pub(crate) fn new() -> Self {
        let state = RecordingAdapterState::default();
        *state.nonce_response.lock().unwrap() = Some(1);
        Self { state: Arc::new(state) }
    }

    // Test modes: 0 missing, -1 RPC error, -2 blocked RPC, 2/3 terminal,
    // 4 nonterminal response, 5 mismatched hash.
    pub(crate) fn receipt_behavior(&self, nonce: u64, mode: i8) {
        self.state.receipt_queries.store(0, Ordering::SeqCst);
        *self.state.receipt_behavior.lock().unwrap() = Some((self.tx_hash(nonce), mode));
    }

    pub(crate) fn receipt_queries(&self) -> usize {
        self.state.receipt_queries.load(Ordering::SeqCst)
    }

    pub(crate) fn canonical_rebroadcast_fixture(&self) {
        self.state.canonical_hash.store(true, Ordering::SeqCst);
        self.state.invisible_transfer.store(true, Ordering::SeqCst);
    }
    pub(crate) fn tx_hash(&self, nonce: u64) -> String {
        if self.state.canonical_hash.load(Ordering::SeqCst) {
            format!("{:#x}", alloy::primitives::keccak256(nonce.to_be_bytes()))
        } else {
            format!("0x{nonce:064x}")
        }
    }
    pub(crate) fn set_visibility(&self, visible: bool, error: bool) {
        self.state.invisible_transfer.store(!visible, Ordering::SeqCst);
        self.state.visibility_error.store(error, Ordering::SeqCst);
    }
    pub(crate) fn build_calls(&self) -> usize {
        self.state.build_calls.load(Ordering::SeqCst)
    }
    pub(crate) fn recorded_raw(&self) -> Vec<Vec<u8>> {
        self.state.recorded_raw.lock().unwrap().clone()
    }

    pub(crate) fn set_nonce_response(&self, response: Option<u64>) {
        *self.state.nonce_response.lock().unwrap() = response;
    }

    pub(crate) fn nonce_queries(&self) -> usize {
        self.state.nonce_queries.load(Ordering::SeqCst)
    }

    pub(crate) fn fail_on_transfer(&self) {
        self.state.fail_transfer.store(true, Ordering::SeqCst);
    }

    pub(crate) fn allow_transfer(&self) {
        self.state.fail_transfer.store(false, Ordering::SeqCst);
        self.state.timeout_transfer.store(false, Ordering::SeqCst);
    }

    pub(crate) fn hide_transfer(&self) {
        self.state.invisible_transfer.store(true, Ordering::SeqCst);
    }

    pub(crate) fn mismatch_hash(&self) {
        self.state.mismatch_hash.store(true, Ordering::SeqCst);
    }

    pub(crate) fn timeout_transfer(&self) {
        self.state.timeout_transfer.store(true, Ordering::SeqCst);
    }

    pub(crate) fn block_first_transfer(&self) {
        self.state.block_first.store(true, Ordering::SeqCst);
    }

    pub(crate) fn recorded_nonces(&self) -> Vec<u64> {
        self.state.recorded_nonces.lock().expect("nonce lock").clone()
    }

    pub(crate) async fn wait_for_first_entry(&self) {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if self.state.first_entered.load(Ordering::SeqCst) {
                    return;
                }
                self.state.entered_notify.notified().await;
            }
        })
        .await
        .expect("first transfer did not reach the adapter");
    }

    pub(crate) fn release_first(&self) {
        self.state.release_first.notify_waiters();
    }
    async fn send_nonce(
        &self,
        nonce: u64,
    ) -> Result<wallet_api::domain::chain::TransferResp, ServiceError> {
        let call_idx = self.state.call_count.fetch_add(1, Ordering::SeqCst);
        {
            let mut nonces = self.state.recorded_nonces.lock().expect("nonce lock");
            nonces.push(nonce);
        }

        if call_idx == 0 && self.state.block_first.load(Ordering::SeqCst) {
            self.state.first_entered.store(true, Ordering::SeqCst);
            self.state.entered_notify.notify_waiters();
            self.state.release_first.notified().await;
        }

        if self.state.timeout_transfer.load(Ordering::SeqCst) {
            return Err(ServiceError::Timeout);
        }
        if self.state.fail_transfer.load(Ordering::SeqCst) {
            return Err(ServiceError::Parameter("simulated transfer failure".to_string()));
        }

        self.set_nonce_response(Some(nonce + 1));
        if self.state.canonical_hash.load(Ordering::SeqCst) {
            self.state.invisible_transfer.store(false, Ordering::SeqCst);
        }
        Ok(wallet_api::domain::chain::TransferResp::new(
            if self.state.mismatch_hash.load(Ordering::SeqCst) {
                "wrong-hash".into()
            } else {
                self.tx_hash(nonce)
            },
            "0".to_string(),
        ))
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
        self.state.nonce_queries.fetch_add(1, Ordering::SeqCst);
        self.state
            .nonce_response
            .lock()
            .unwrap()
            .ok_or_else(|| ServiceError::Parameter("simulated nonce RPC failure".into()))
    }

    async fn block_num(&self) -> Result<u64, ChainError> {
        Ok(0)
    }

    async fn query_tx_res(&self, hash: &str) -> Result<Option<QueryTransactionResult>, ChainError> {
        let behavior = self.state.receipt_behavior.lock().unwrap().clone();
        let Some((expected, mode)) = behavior else {
            return Ok(None);
        };
        if hash != expected {
            return Ok(None);
        }
        self.state.receipt_queries.fetch_add(1, Ordering::SeqCst);
        match mode {
            0 => Ok(None),
            -1 => Err(ChainError::Other("simulated receipt RPC failure".into())),
            -2 => std::future::pending().await,
            _ => Ok(Some(QueryTransactionResult::new(
                if mode == 5 { "wrong-hash".into() } else { hash.into() },
                0.00002,
                "21000".into(),
                1_700_000_000,
                if mode == 5 { 2 } else { mode },
                1234,
            ))),
        }
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
        self.send_nonce(params.nonce).await
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
        params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<(String, wallet_api::domain::api_wallet::RawTx, String), ServiceError> {
        self.state.build_calls.fetch_add(1, Ordering::SeqCst);
        Ok((
            self.tx_hash(params.nonce),
            wallet_api::domain::api_wallet::RawTx::Evm(
                params.nonce.to_be_bytes().to_vec(),
                U256::ZERO,
            ),
            "0".to_string(),
        ))
    }

    async fn broadcast_transfer(
        &self,
        raw: wallet_api::domain::api_wallet::RawTx,
    ) -> Result<wallet_api::domain::chain::TransferResp, ServiceError> {
        let wallet_api::domain::api_wallet::RawTx::Evm(bytes, _) = raw else {
            panic!("expected EVM raw")
        };
        self.state.recorded_raw.lock().unwrap().push(bytes.clone());
        self.send_nonce(u64::from_be_bytes(bytes.try_into().unwrap())).await
    }

    async fn query_tx_seen_on_node(&self, _hash: &str) -> Result<bool, ServiceError> {
        if self.state.visibility_error.load(Ordering::SeqCst) {
            return Err(ServiceError::Timeout);
        }
        Ok(!self.state.invisible_transfer.load(Ordering::SeqCst))
    }
}

pub(crate) struct AdapterGuard {
    chain_code: String,
}

impl Drop for AdapterGuard {
    fn drop(&mut self) {
        clear_test_transaction_adapter_override(&self.chain_code);
    }
}

pub(crate) fn install_adapter(adapter: &RecordingEthAdapter) -> AdapterGuard {
    let chain_code = "bnb".to_string();
    let adapter: Arc<dyn wallet_api::domain::api_wallet::Tx + Send + Sync> =
        Arc::new(adapter.clone());
    set_test_transaction_adapter_override(&chain_code, adapter);
    AdapterGuard { chain_code }
}
