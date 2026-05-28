use crate::{
    error::service::ServiceError, infrastructure::api_trans::collect_fee::shadow::ShadowFeeWorker,
    request::api_wallet::trans::ApiBaseTransferReq,
};

/// Test-facing wrapper for the Solana fee rent helper.
///
/// Keeping this in `test_support` allows integration tests to exercise the
/// behavior without embedding test code inside the deprecated fee worker module.
pub async fn bump_sol_native_transfer_value_for_rent(
    params: &mut ApiBaseTransferReq,
    symbol: &str,
    trade_no: &str,
) -> Result<(), ServiceError> {
    ShadowFeeWorker::bump_sol_native_transfer_value_for_rent(params, symbol, trade_no).await
}

/// Test-facing wrapper for the Solana rent error detector.
pub fn is_solana_recipient_rent_error(err: &ServiceError) -> bool {
    ShadowFeeWorker::is_solana_recipient_rent_error(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::api_wallet::{RawTx, Tx},
        error::business::{
            BusinessError,
            chain::{ChainError, InsufficientBalanceDetail},
        },
        request::api_wallet::trans::ApiBaseTransferReq,
        testkit::adapter_factory::{
            clear_test_transaction_adapter_override, set_test_transaction_adapter_override,
        },
    };
    use alloy::primitives::U256;
    use serial_test::serial;
    use std::sync::Arc;
    use wallet_chain_interact::{Error, QueryTransactionResult};
    use wallet_database::entities::asset_token_key::AssetTokenKey;

    #[derive(Clone)]
    struct RentAwareAdapter {
        return_rent_error: bool,
    }

    #[async_trait::async_trait]
    impl Tx for RentAwareAdapter {
        async fn account_resource(
            &self,
            _owner_address: &str,
        ) -> Result<
            wallet_chain_interact::tron::protocol::account::AccountResourceDetail,
            ServiceError,
        > {
            unimplemented!()
        }

        async fn balance_token_key(
            &self,
            _addr: &str,
            _token: AssetTokenKey,
        ) -> Result<U256, Error> {
            Ok(U256::ZERO)
        }

        async fn nonce(&self, _addr: &str) -> Result<u64, ServiceError> {
            Ok(0)
        }

        async fn block_num(&self) -> Result<u64, Error> {
            Ok(0)
        }

        async fn query_tx_res(&self, _hash: &str) -> Result<Option<QueryTransactionResult>, Error> {
            Ok(None)
        }

        async fn token_symbol(&self, _token: &str) -> Result<String, Error> {
            Ok("SOL".to_string())
        }

        async fn token_name(&self, _token: &str) -> Result<String, Error> {
            Ok("Solana".to_string())
        }

        async fn decimals(&self, _token: &str) -> Result<u8, Error> {
            Ok(9)
        }

        async fn black_address(&self, _token: &str, _owner: &str) -> Result<bool, ServiceError> {
            Ok(false)
        }

        async fn transfer(
            &self,
            _params: &crate::request::api_wallet::trans::ApiTransferReq,
            _private_key: wallet_chain_interact::types::ChainPrivateKey,
        ) -> Result<crate::domain::chain::TransferResp, ServiceError> {
            unimplemented!()
        }

        async fn estimate_fee(
            &self,
            req: ApiBaseTransferReq,
            _main_symbol: &str,
        ) -> Result<String, ServiceError> {
            if self.return_rent_error && req.token_address.is_native() {
                return Err(ServiceError::Business(BusinessError::Chain(
                    ChainError::insufficient_balance_with_detail(
                        InsufficientBalanceDetail::new().reason(
                            "recipient account is not initialized and transfer amount is below rent-exempt minimum",
                        ),
                    ),
                )));
            }

            Ok(r#"{"estimateFee":{"amount":"0.000015","currency":"USD","unitPrice":0.0,"fiatValue":0.0}}"#
                .to_string())
        }

        async fn build_transfer_raw(
            &self,
            _params: &crate::request::api_wallet::trans::ApiTransferReq,
            _private_key: wallet_chain_interact::types::ChainPrivateKey,
        ) -> Result<(String, RawTx, String), ServiceError> {
            unimplemented!()
        }

        async fn broadcast_transfer(
            &self,
            _raw: RawTx,
        ) -> Result<crate::domain::chain::TransferResp, ServiceError> {
            unimplemented!()
        }
    }

    struct AdapterGuard;

    impl Drop for AdapterGuard {
        fn drop(&mut self) {
            clear_test_transaction_adapter_override("sol");
        }
    }

    fn install_adapter(return_rent_error: bool) -> AdapterGuard {
        let adapter: Arc<dyn Tx + Send + Sync> = Arc::new(RentAwareAdapter { return_rent_error });
        set_test_transaction_adapter_override("sol", adapter);
        AdapterGuard
    }

    #[test]
    fn solana_recipient_rent_error_detector_matches_precheck_message() {
        let err = ServiceError::Business(BusinessError::Chain(
            ChainError::insufficient_balance_with_detail(InsufficientBalanceDetail::new().reason(
                "recipient account is not initialized and transfer amount is below rent-exempt minimum",
            )),
        ));

        assert!(is_solana_recipient_rent_error(&err));
    }

    #[test]
    fn solana_recipient_rent_error_detector_ignores_other_errors() {
        let err = ServiceError::Parameter("other error".into());
        assert!(!is_solana_recipient_rent_error(&err));
    }

    #[serial]
    #[tokio::test]
    async fn bump_sol_native_transfer_value_for_rent_increases_value_when_rent_error_is_detected() {
        let _guard = install_adapter(true);
        let mut params = ApiBaseTransferReq::new(
            "from",
            "to",
            "0.000015",
            &wallet_types::chain::chain::ChainCode::Solana.to_string(),
        );
        params.with_token(AssetTokenKey::Native, 9, "SOL");

        bump_sol_native_transfer_value_for_rent(&mut params, "SOL", "T1").await.expect("bump rent");

        let bumped = wallet_utils::conversion::decimal_from_str(&params.value).unwrap();
        let original = wallet_utils::conversion::decimal_from_str("0.000015").unwrap();
        assert!(bumped > original);
    }

    #[serial]
    #[tokio::test]
    async fn bump_sol_native_transfer_value_for_rent_keeps_value_when_estimate_succeeds() {
        let _guard = install_adapter(false);
        let mut params = ApiBaseTransferReq::new(
            "from",
            "to",
            "1.0",
            &wallet_types::chain::chain::ChainCode::Solana.to_string(),
        );
        params.with_token(AssetTokenKey::Contract("mint".to_string()), 6, "USDC");

        bump_sol_native_transfer_value_for_rent(&mut params, "USDC", "T2")
            .await
            .expect("no bump needed");

        assert_eq!(params.value, "1.0");
    }
}
