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
    call_count: AtomicUsize,
    first_entered: AtomicBool,
    fail_transfer: AtomicBool,
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
        Self { state: Arc::new(RecordingAdapterState::default()) }
    }

    pub(crate) fn fail_on_transfer(&self) {
        self.state.fail_transfer.store(true, Ordering::SeqCst);
    }

    pub(crate) fn block_first_transfer(&self) {
        self.state.block_first.store(true, Ordering::SeqCst);
    }

    pub(crate) fn recorded_nonces(&self) -> Vec<u64> {
        self.state.recorded_nonces.lock().expect("nonce lock").clone()
    }

    pub(crate) async fn wait_for_first_entry(&self) {
        loop {
            if self.state.first_entered.load(Ordering::SeqCst) {
                return;
            }
            self.state.entered_notify.notified().await;
        }
    }

    pub(crate) fn release_first(&self) {
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
