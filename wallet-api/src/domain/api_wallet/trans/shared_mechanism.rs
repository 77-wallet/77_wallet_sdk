use crate::{
    context::Context,
    domain::api_wallet::{
        adapter::tx::TxVisibilityProbe, adapter_factory::ApiChainAdapterFactory,
        trans::broadcast_mechanism::BroadcastAcceptedReason,
    },
    error::service::ServiceError,
    infrastructure::{chain_rpc_guard, nonce::nonce_engine::get_nonce_engine_with_ctx},
};
use std::fmt::Display;
use wallet_types::chain::chain::ChainCode;

pub(crate) fn is_evm_chain(chain_code: &str) -> bool {
    chain_code == ChainCode::Ethereum.to_string()
        || chain_code == ChainCode::BnbSmartChain.to_string()
}

pub(crate) fn is_sol_chain(chain_code: &str) -> bool {
    chain_code == ChainCode::Solana.to_string()
}

pub(crate) fn need_broadcast_visibility_check(chain_code: &str) -> bool {
    is_evm_chain(chain_code) || is_sol_chain(chain_code)
}

pub(crate) fn warn_chain_rpc_failure(
    error: &impl Display,
    log: chain_rpc_guard::ChainRpcFailureLog<'_>,
    reason: &str,
) -> chain_rpc_guard::ChainRpcErrorContext {
    let rpc_error = chain_rpc_guard::ChainRpcErrorContext::from_display(error);
    log.warn(&rpc_error, reason);
    rpc_error.record_transient_failure();
    rpc_error
}

pub(crate) fn error_chain_rpc_failure(
    error: &impl Display,
    log: chain_rpc_guard::ChainRpcFailureLog<'_>,
    reason: &str,
) -> chain_rpc_guard::ChainRpcErrorContext {
    let rpc_error = chain_rpc_guard::ChainRpcErrorContext::from_display(error);
    log.error(&rpc_error, reason);
    rpc_error.record_transient_failure();
    rpc_error
}

pub(crate) fn log_visibility_probe(
    chain_code: &str,
    tx_hash: &str,
    rpc: &str,
    visibility_kind: &str,
    probe: &TxVisibilityProbe,
    attempt: Option<usize>,
    delay_ms: Option<u64>,
    message: &str,
) {
    let signature_status = probe.signature_status.as_deref().unwrap_or("missing");
    let transaction_result = probe.transaction_result.as_deref().unwrap_or("missing");
    let health = probe.health.as_deref().unwrap_or("missing");
    let visibility_reason = probe.visibility_reason.as_deref().unwrap_or("missing");

    match (attempt, delay_ms) {
        (Some(attempt), Some(delay_ms)) => tracing::info!(
            chain_code = %chain_code,
            tx_hash = %tx_hash,
            rpc = %rpc,
            signature_status = %signature_status,
            transaction_result = %transaction_result,
            health = %health,
            slot = ?probe.slot,
            visibility_reason = %visibility_reason,
            attempt = attempt,
            delay_ms = delay_ms,
            visibility_kind = %visibility_kind,
            message = %message,
            "broadcast visibility probe"
        ),
        _ => tracing::info!(
            chain_code = %chain_code,
            tx_hash = %tx_hash,
            rpc = %rpc,
            signature_status = %signature_status,
            transaction_result = %transaction_result,
            health = %health,
            slot = ?probe.slot,
            visibility_reason = %visibility_reason,
            visibility_kind = %visibility_kind,
            message = %message,
            "broadcast visibility probe"
        ),
    }
}

pub(crate) fn normalize_chain_time_to_ms(raw: u128) -> Result<u128, ServiceError> {
    const SECOND_THRESHOLD: u128 = 100_000_000_000; // < 1e11 视为秒
    const SEC_TO_MS: u128 = 1000;

    if raw == 0 {
        return Err(ServiceError::System(crate::error::system::SystemError::Internal(
            "invalid chain transaction_time: 0".to_string(),
        )));
    }

    let normalized = if raw < SECOND_THRESHOLD {
        raw.checked_mul(SEC_TO_MS).ok_or_else(|| {
            ServiceError::System(crate::error::system::SystemError::Internal(
                "chain transaction_time seconds->milliseconds overflow".to_string(),
            ))
        })?
    } else {
        raw
    };

    if normalized > i64::MAX as u128 {
        return Err(ServiceError::System(crate::error::system::SystemError::Internal(
            "chain transaction_time out of range for DateTime::from_timestamp_millis".to_string(),
        )));
    }

    Ok(normalized)
}

pub(crate) async fn refresh_rpc_auth_and_prepare_retry(
    ctx: &'static Context,
    chain_code: &str,
    op: &str,
    rpc: Option<&str>,
    err: &ServiceError,
) -> Result<(), ServiceError> {
    let rpc = rpc.unwrap_or("<unknown>");
    tracing::warn!(
        chain_code = %chain_code,
        op = %op,
        rpc = %rpc,
        error = %err,
        retry = "auth_refresh_once",
        "rpc auth unauthorized detected"
    );

    ctx.invalidate_rpc_token_cache().await;

    tracing::warn!(
        chain_code = %chain_code,
        op = %op,
        rpc = %rpc,
        client_id = %ctx.get_client_id(),
        "force refresh rpc token"
    );
    match ctx.get_rpc_header_force_refresh().await {
        Ok(_) => {
            tracing::info!(
                chain_code = %chain_code,
                op = %op,
                rpc = %rpc,
                client_id = %ctx.get_client_id(),
                "force refresh rpc token success"
            );
        }
        Err(refresh_err) => {
            tracing::error!(
                chain_code = %chain_code,
                op = %op,
                rpc = %rpc,
                error = %refresh_err,
                "force refresh rpc token failed"
            );
            return Err(refresh_err);
        }
    }

    let removed_count = ApiChainAdapterFactory::invalidate_all_cached_adapters();
    tracing::warn!(
        chain_code = %chain_code,
        op = %op,
        rpc = %rpc,
        removed_count = removed_count,
        "invalidate cached adapters after rpc auth refresh"
    );
    Ok(())
}

pub(crate) async fn try_nonce_self_heal(
    ctx: &'static Context,
    trade_no: &str,
    from_addr: &str,
    chain_code: &str,
    error_msg: &str,
    source: &str,
) -> Result<(), ServiceError> {
    let nonce_engine = get_nonce_engine_with_ctx(ctx)?;
    if let Err(err) = nonce_engine.handle_nonce_error(from_addr, chain_code, error_msg).await {
        tracing::warn!(
            trade_no = %trade_no,
            error = %err,
            source = %source,
            "Nonce self-heal failed"
        );
    }
    Ok(())
}

pub(crate) async fn handle_accepted_broadcast_nonce_self_heal(
    ctx: &'static Context,
    trade_no: &str,
    from_addr: &str,
    chain_code: &str,
    error_msg: &str,
    source: &str,
    accepted_reason: BroadcastAcceptedReason,
) -> Result<(), ServiceError> {
    if let Some(message) = accepted_reason.pre_self_heal_log_message() {
        tracing::info!(
            trade_no = %trade_no,
            source = %source,
            "{}", message
        );
    }

    if accepted_reason.requires_nonce_self_heal() {
        try_nonce_self_heal(ctx, trade_no, from_addr, chain_code, error_msg, source).await?;
    }

    Ok(())
}

pub(crate) async fn handle_nonce_too_low_before_terminal_fail(
    ctx: &'static Context,
    trade_no: &str,
    from_addr: &str,
    chain_code: &str,
    error_msg: &str,
    source: &str,
) -> Result<(), ServiceError> {
    tracing::info!(
        trade_no = %trade_no,
        source = %source,
        "Detected nonce too low error, syncing nonce from chain"
    );

    try_nonce_self_heal(ctx, trade_no, from_addr, chain_code, error_msg, source).await
}

#[cfg(test)]
mod tests {
    use super::normalize_chain_time_to_ms;

    #[test]
    fn normalize_chain_time_seconds_to_ms() {
        let sec = 1_746_040_371_u128; // 2025-05-xx second-level timestamp
        let ms = normalize_chain_time_to_ms(sec).unwrap();
        assert_eq!(ms, sec * 1000);
    }

    #[test]
    fn normalize_chain_time_milliseconds_kept_as_is() {
        let ms = 1_746_040_371_123_u128;
        let normalized = normalize_chain_time_to_ms(ms).unwrap();
        assert_eq!(normalized, ms);
    }

    #[test]
    fn normalize_chain_time_zero_should_fail() {
        assert!(normalize_chain_time_to_ms(0).is_err());
    }
}
