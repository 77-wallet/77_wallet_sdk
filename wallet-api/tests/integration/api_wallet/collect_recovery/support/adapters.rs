use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use alloy::primitives::U256;
use serde_json::json;
use wallet_api::{
    domain::api_wallet::{RawTx, Tx},
    testkit::adapter_factory::{
        clear_test_transaction_adapter_override, set_test_transaction_adapter_override,
    },
};
use wallet_chain_interact::{
    BillResourceConsume, QueryTransactionResult, tron::operations::RawTransactionParams,
};
use wallet_database::entities::asset_token_key::AssetTokenKey;
use wallet_types::chain::chain::ChainCode;

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

pub(crate) struct TronRecoverProbeGuard {
    chain_code: String,
}

impl Drop for TronRecoverProbeGuard {
    fn drop(&mut self) {
        clear_test_transaction_adapter_override(&self.chain_code);
    }
}

pub(crate) fn install_collect_tron_recover_probe_adapter(
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

pub(crate) fn expired_tron_raw_tx_json(expiration_ms: i64) -> String {
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
