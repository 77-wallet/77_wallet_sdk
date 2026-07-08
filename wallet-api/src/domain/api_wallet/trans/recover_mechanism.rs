use crate::{error::service::ServiceError, infrastructure::chain_rpc_guard};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::RetryableError as _;

pub(super) const RECOVER_OP_GET_ADAPTER: &str = "process_recovered_tx:get_adapter";
pub(super) const RECOVER_OP_QUERY_TX_RES: &str = "process_recovered_tx:query_tx_res";
pub(super) const RECOVER_OP_SOL_VISIBILITY_PROBE: &str =
    "process_recovered_tx:sol_visibility_probe";
pub(super) const RECOVER_OP_TRON_PENDING_POOL: &str = "process_recovered_tx:tron_pending_pool";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoverAdapterErrorDecision {
    RefreshAuthAndRetry,
    StayUncertain,
    Fatal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoverQueryErrorDecision {
    RefreshAuthAndRetry,
    StayUncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvmRecoverMissingTxHashState {
    /// The chain nonce has advanced past the locally recorded nonce.
    /// This means the recorded hash is no longer the active pending tx,
    /// but it does not prove final failure.
    ChainNonceConsumedLocalHashMissing,
    /// The chain nonce still matches the next expected nonce.
    /// The tx may still be in flight, dropped, or only accepted by the RPC.
    ChainNonceMatchesLocalNonce,
    /// Local nonce is ahead of the chain nonce. This usually means the local
    /// snapshot is stale or another flow has not yet advanced the chain.
    LocalNonceAheadOfChainNonce,
}

impl EvmRecoverMissingTxHashState {
    pub(super) fn uncertain_reason(self) -> &'static str {
        match self {
            EvmRecoverMissingTxHashState::ChainNonceConsumedLocalHashMissing => {
                "EVM recover: local tx hash missing but nonce already consumed on-chain; keep uncertain"
            }
            EvmRecoverMissingTxHashState::ChainNonceMatchesLocalNonce => {
                "EVM recover: nonce matches next expected nonce but tx hash missing (rpc accepted only / not propagated / dropped)"
            }
            EvmRecoverMissingTxHashState::LocalNonceAheadOfChainNonce => {
                "EVM recover: local nonce ahead of chain nonce (future nonce / nonce gap suspected)"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoverMissingTxRoute {
    SolanaVisibilityProbe,
    TronPendingPool,
    EvmNonceCheck,
    WaitForScanner,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct RecoverAuthRetryState {
    attempted: bool,
}

impl RecoverAuthRetryState {
    pub(super) fn attempted(&self) -> bool {
        self.attempted
    }

    pub(super) fn mark_attempted(&mut self) {
        self.attempted = true;
    }

    pub(super) fn classify_adapter_error(&self, err: &ServiceError) -> RecoverAdapterErrorDecision {
        classify_recover_adapter_error(err, self.attempted)
    }

    pub(super) fn classify_query_error(&self, err: &ServiceError) -> RecoverQueryErrorDecision {
        classify_recover_query_error(err, self.attempted)
    }
}

pub(super) fn classify_evm_missing_tx_hash_state(
    chain_nonce: u64,
    local_nonce: u64,
) -> EvmRecoverMissingTxHashState {
    use std::cmp::Ordering;

    match chain_nonce.cmp(&local_nonce) {
        Ordering::Greater => EvmRecoverMissingTxHashState::ChainNonceConsumedLocalHashMissing,
        Ordering::Equal => EvmRecoverMissingTxHashState::ChainNonceMatchesLocalNonce,
        Ordering::Less => EvmRecoverMissingTxHashState::LocalNonceAheadOfChainNonce,
    }
}

pub(super) fn classify_recover_missing_tx_route(chain_code: &str) -> RecoverMissingTxRoute {
    if chain_code == ChainCode::Solana.to_string() {
        RecoverMissingTxRoute::SolanaVisibilityProbe
    } else if chain_code == ChainCode::Tron.to_string() {
        RecoverMissingTxRoute::TronPendingPool
    } else if chain_code == ChainCode::Ethereum.to_string()
        || chain_code == ChainCode::BnbSmartChain.to_string()
    {
        RecoverMissingTxRoute::EvmNonceCheck
    } else {
        RecoverMissingTxRoute::WaitForScanner
    }
}

pub(super) fn classify_recover_adapter_error(
    err: &ServiceError,
    auth_retry_attempted: bool,
) -> RecoverAdapterErrorDecision {
    if !auth_retry_attempted && err.is_rpc_auth_unauthorized() {
        RecoverAdapterErrorDecision::RefreshAuthAndRetry
    } else if err.is_delay_retryable() {
        RecoverAdapterErrorDecision::StayUncertain
    } else {
        RecoverAdapterErrorDecision::Fatal
    }
}

pub(super) fn classify_recover_query_error(
    err: &ServiceError,
    auth_retry_attempted: bool,
) -> RecoverQueryErrorDecision {
    if !auth_retry_attempted && err.is_rpc_auth_unauthorized() {
        RecoverQueryErrorDecision::RefreshAuthAndRetry
    } else {
        RecoverQueryErrorDecision::StayUncertain
    }
}

pub(super) fn recover_failure_log<'a>(
    chain_code: &'a str,
    tx_hash: &'a str,
    rpc: &'a str,
    op: &'a str,
) -> chain_rpc_guard::ChainRpcFailureLog<'a> {
    chain_rpc_guard::ChainRpcFailureLog::new()
        .trade_no(tx_hash)
        .tx_hash(tx_hash)
        .chain_code(chain_code)
        .rpc(rpc)
        .op(op)
}

pub(super) fn log_recover_query_error(
    chain_code: &str,
    tx_hash: &str,
    rpc: &str,
    service_err: &ServiceError,
    auth_retry_attempted: bool,
) {
    let failure_log = recover_failure_log(chain_code, tx_hash, rpc, RECOVER_OP_QUERY_TX_RES);
    let rpc_error = chain_rpc_guard::ChainRpcErrorContext::from_display(service_err);
    if rpc_error.is_transient() {
        failure_log
            .warn(&rpc_error, "查询链上状态失败(瞬时); recovered tx result will stay uncertain");
    } else {
        failure_log.error(&rpc_error, "查询链上状态失败; recovered tx result will stay uncertain");
    }
    rpc_error.record_transient_failure();
    if auth_retry_attempted {
        tracing::warn!(chain_code=%chain_code, rpc=%rpc, op=RECOVER_OP_QUERY_TX_RES, error=%service_err, "auth retry failed");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EvmRecoverMissingTxHashState, RecoverAdapterErrorDecision, RecoverAuthRetryState,
        RecoverMissingTxRoute, RecoverQueryErrorDecision, classify_evm_missing_tx_hash_state,
        classify_recover_adapter_error, classify_recover_missing_tx_route,
        classify_recover_query_error, log_recover_query_error,
    };
    use crate::error::service::ServiceError;
    use wallet_types::chain::chain::ChainCode;

    #[test]
    fn evm_recover_missing_tx_hash_treats_nonce_consumed_as_uncertain() {
        let state = classify_evm_missing_tx_hash_state(4, 3);
        assert_eq!(state, EvmRecoverMissingTxHashState::ChainNonceConsumedLocalHashMissing);
        assert!(state.uncertain_reason().contains("nonce already consumed"));
    }

    #[test]
    fn evm_recover_missing_tx_hash_detects_equal_nonce_as_uncertain() {
        let state = classify_evm_missing_tx_hash_state(3, 3);
        assert_eq!(state, EvmRecoverMissingTxHashState::ChainNonceMatchesLocalNonce);
        assert!(state.uncertain_reason().contains("nonce matches"));
    }

    #[test]
    fn evm_recover_missing_tx_hash_detects_local_nonce_ahead() {
        let state = classify_evm_missing_tx_hash_state(2, 3);
        assert_eq!(state, EvmRecoverMissingTxHashState::LocalNonceAheadOfChainNonce);
        assert!(state.uncertain_reason().contains("local nonce ahead"));
    }

    #[test]
    fn recover_adapter_unauthorized_refreshes_auth_before_retry() {
        let err = ServiceError::TransportBackend(wallet_transport_backend::Error::ApiBackend(
            401,
            Some("Unauthorized".to_string()),
        ));

        let decision = classify_recover_adapter_error(&err, false);

        assert_eq!(decision, RecoverAdapterErrorDecision::RefreshAuthAndRetry);
    }

    #[test]
    fn recover_adapter_unauthorized_after_auth_retry_is_fatal() {
        let err = ServiceError::TransportBackend(wallet_transport_backend::Error::ApiBackend(
            401,
            Some("Unauthorized".to_string()),
        ));

        let decision = classify_recover_adapter_error(&err, true);

        assert_eq!(decision, RecoverAdapterErrorDecision::Fatal);
    }

    #[test]
    fn recover_adapter_rate_limited_stays_uncertain() {
        let err = ServiceError::TransportBackend(wallet_transport_backend::Error::RateLimited);

        let decision = classify_recover_adapter_error(&err, false);

        assert_eq!(decision, RecoverAdapterErrorDecision::StayUncertain);
    }

    #[test]
    fn recover_query_unauthorized_refreshes_auth_before_retry() {
        let err = ServiceError::TransportBackend(wallet_transport_backend::Error::ApiBackend(
            401,
            Some("Unauthorized".to_string()),
        ));

        let decision = classify_recover_query_error(&err, false);

        assert_eq!(decision, RecoverQueryErrorDecision::RefreshAuthAndRetry);
    }

    #[test]
    fn recover_query_unauthorized_after_auth_retry_stays_uncertain() {
        let err = ServiceError::TransportBackend(wallet_transport_backend::Error::ApiBackend(
            401,
            Some("Unauthorized".to_string()),
        ));

        let decision = classify_recover_query_error(&err, true);

        assert_eq!(decision, RecoverQueryErrorDecision::StayUncertain);
    }

    #[test]
    fn recover_transient_query_error_is_logged_and_stays_uncertain() {
        let err = ServiceError::Parameter(
            "Node response error: code=503, rpc=https://api.nileex.io/wallet/gettransactioninfobyid"
                .to_string(),
        );

        log_recover_query_error("tron", "0xtx", "https://api.nileex.io", &err, false);
    }

    #[test]
    fn recover_non_transient_query_error_is_logged_and_stays_uncertain() {
        let err = ServiceError::Parameter("invalid tx hash".to_string());

        log_recover_query_error("tron", "0xtx", "https://api.nileex.io", &err, true);
    }

    #[test]
    fn recover_auth_retry_state_tracks_single_retry_attempt() {
        let mut state = RecoverAuthRetryState::default();
        assert!(!state.attempted());

        state.mark_attempted();

        assert!(state.attempted());
    }

    #[test]
    fn recover_auth_retry_state_classifies_adapter_error_with_attempt_state() {
        let err = ServiceError::TransportBackend(wallet_transport_backend::Error::ApiBackend(
            401,
            Some("Unauthorized".to_string()),
        ));
        let mut state = RecoverAuthRetryState::default();

        assert_eq!(
            state.classify_adapter_error(&err),
            RecoverAdapterErrorDecision::RefreshAuthAndRetry
        );

        state.mark_attempted();

        assert_eq!(state.classify_adapter_error(&err), RecoverAdapterErrorDecision::Fatal);
    }

    #[test]
    fn recover_auth_retry_state_classifies_query_error_with_attempt_state() {
        let err = ServiceError::TransportBackend(wallet_transport_backend::Error::ApiBackend(
            401,
            Some("Unauthorized".to_string()),
        ));
        let mut state = RecoverAuthRetryState::default();

        assert_eq!(
            state.classify_query_error(&err),
            RecoverQueryErrorDecision::RefreshAuthAndRetry
        );

        state.mark_attempted();

        assert_eq!(state.classify_query_error(&err), RecoverQueryErrorDecision::StayUncertain);
    }

    #[test]
    fn recover_missing_tx_routes_solana_to_visibility_probe() {
        let route = classify_recover_missing_tx_route(&ChainCode::Solana.to_string());
        assert_eq!(route, RecoverMissingTxRoute::SolanaVisibilityProbe);
    }

    #[test]
    fn recover_missing_tx_routes_tron_to_pending_pool() {
        let route = classify_recover_missing_tx_route(&ChainCode::Tron.to_string());
        assert_eq!(route, RecoverMissingTxRoute::TronPendingPool);
    }

    #[test]
    fn recover_missing_tx_routes_evm_chains_to_nonce_check() {
        for chain_code in [ChainCode::Ethereum.to_string(), ChainCode::BnbSmartChain.to_string()] {
            let route = classify_recover_missing_tx_route(&chain_code);
            assert_eq!(route, RecoverMissingTxRoute::EvmNonceCheck);
        }
    }

    #[test]
    fn recover_missing_tx_routes_unknown_chain_to_wait() {
        let route = classify_recover_missing_tx_route("aptos");
        assert_eq!(route, RecoverMissingTxRoute::WaitForScanner);
    }
}
