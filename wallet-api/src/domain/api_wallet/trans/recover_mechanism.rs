use crate::{error::service::ServiceError, infrastructure::chain_rpc_guard};
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

#[cfg(test)]
mod tests {
    use super::{
        EvmRecoverMissingTxHashState, RecoverAdapterErrorDecision, RecoverQueryErrorDecision,
        classify_evm_missing_tx_hash_state, classify_recover_adapter_error,
        classify_recover_query_error,
    };
    use crate::error::service::ServiceError;

    #[test]
    fn evm_recover_missing_tx_hash_treats_nonce_consumed_as_uncertain() {
        let state = classify_evm_missing_tx_hash_state(4, 3);
        assert_eq!(state, EvmRecoverMissingTxHashState::ChainNonceConsumedLocalHashMissing);
    }

    #[test]
    fn evm_recover_missing_tx_hash_detects_equal_nonce_as_uncertain() {
        let state = classify_evm_missing_tx_hash_state(3, 3);
        assert_eq!(state, EvmRecoverMissingTxHashState::ChainNonceMatchesLocalNonce);
    }

    #[test]
    fn evm_recover_missing_tx_hash_detects_local_nonce_ahead() {
        let state = classify_evm_missing_tx_hash_state(2, 3);
        assert_eq!(state, EvmRecoverMissingTxHashState::LocalNonceAheadOfChainNonce);
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
}
