use crate::{
    context::Context,
    domain::{
        api_wallet::{
            account::ApiAccountDomain,
            adapter::tx::{RawTx, Tx, TxVisibilityProbe},
            adapter_factory::ApiChainAdapterFactory,
        },
        chain::TransferResp,
    },
    error::service::ServiceError,
    infrastructure::chain_rpc_guard,
    request::api_wallet::trans::ApiTransferReq,
};
use sha3::{Digest, Keccak256};
use std::{fmt::Display, sync::Arc, time::Instant};
use tokio::time::sleep;
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_types::chain::chain::ChainCode;
use wallet_utils::RetryableError as _;

pub(crate) mod collect;
pub(crate) mod fee;
mod recover_mechanism;
pub(crate) mod withdraw;

use recover_mechanism::{
    RECOVER_OP_GET_ADAPTER, RECOVER_OP_QUERY_TX_RES, RecoverAdapterErrorDecision,
    RecoverAuthRetryState, RecoverMissingTxRoute, RecoverQueryErrorDecision, RecoverQueryOutcome,
    classify_recover_missing_tx_route, log_evm_missing_tx_hash_state, log_recover_query_error,
    log_sol_visibility_probe_error, log_tron_pending_pool_query_error, recover_query_outcome,
};

enum RecoverAdapterAcquire {
    Ready(Arc<dyn Tx + Send + Sync>),
    RetryAuth,
    StayUncertain,
}

enum RecoverQueryErrorAction {
    RetryAuth,
    StayUncertain,
}

#[cfg(test)]
mod confirm_tx_tests;

#[cfg(test)]
mod sol_broadcast_error_tests {
    use super::ApiTransDomain;
    use crate::{domain::api_wallet::adapter::tx::RawTx, error::service::ServiceError};

    #[test]
    fn duplicate_processed_should_be_treated_as_duplicate() {
        let err = ServiceError::Parameter(
            "Transaction simulation failed: This transn has already been processed".to_string(),
        );
        assert!(ApiTransDomain::is_duplicate_broadcast_error(&err));
        assert!(!ApiTransDomain::is_recoverable_sol_broadcast_error(&err));
    }

    #[test]
    fn blockhash_not_found_should_not_be_recoverable_simulation_failure() {
        let err = ServiceError::Parameter(
            "node response: Transaction simulation failed: Blockhash not found".to_string(),
        );
        assert!(ApiTransDomain::is_blockhash_not_found_error(&err));
        assert!(!ApiTransDomain::is_recoverable_sol_broadcast_error(&err));
    }

    #[test]
    fn tron_transaction_expired_error_is_detected() {
        let err = ServiceError::Parameter(
            "Chain interact error: `rpc error Transaction expired`".to_string(),
        );
        assert!(ApiTransDomain::is_transaction_expired_error(&err));
        assert!(!ApiTransDomain::is_blockhash_not_found_error(&err));
    }

    #[test]
    fn tron_transaction_expired_invalidates_raw_for_rebuild() {
        let err = ServiceError::Parameter(
            "Chain interact error: `rpc error Transaction expired`".to_string(),
        );

        assert!(ApiTransDomain::should_invalidate_tron_expired_broadcast(
            "tron", &err, true, true, false,
        ));
        assert!(!ApiTransDomain::should_invalidate_tron_expired_broadcast(
            "tron", &err, true, true, true,
        ));
        assert!(!ApiTransDomain::should_invalidate_tron_expired_broadcast(
            "tron", &err, true, false, false,
        ));
        assert!(!ApiTransDomain::should_invalidate_tron_expired_broadcast(
            "sol", &err, true, true, false,
        ));
    }

    #[test]
    fn rent_shortage_should_not_be_recoverable_simulation_failure() {
        let err = ServiceError::Parameter(
            "node response: Transaction simulation failed: Transaction results in an account (0) with insufficient funds for rent"
                .to_string(),
        );
        assert!(!ApiTransDomain::is_recoverable_sol_broadcast_error(&err));
    }

    #[test]
    fn generic_sol_simulation_failure_should_fail_fast() {
        let err = ServiceError::Parameter(
            "node response: Transaction simulation failed: custom program error: 0x1".to_string(),
        );
        assert!(!ApiTransDomain::is_recoverable_sol_broadcast_error(&err));
    }

    #[test]
    fn duplicate_tx_hash_should_prefer_expected_hash() {
        let picked = ApiTransDomain::resolve_duplicate_tx_hash(
            Some("  expected_tx_hash  "),
            Some("fallback"),
        );
        assert_eq!(picked.as_deref(), Some("expected_tx_hash"));
    }

    #[test]
    fn sol_tx_hash_hint_must_not_use_raw_sol_payload_field() {
        let raw = RawTx::Sol("not_a_signature".to_string(), "serialized_or_fee".to_string());
        let hint = ApiTransDomain::select_broadcast_tx_hash_hint(&raw, Some("real_sol_signature"));
        assert_eq!(hint.as_deref(), Some("real_sol_signature"));

        let missing = ApiTransDomain::select_broadcast_tx_hash_hint(&raw, None);
        assert!(missing.is_none());
    }

    #[test]
    fn normalize_chain_time_seconds_to_ms() {
        let sec = 1_746_040_371_u128; // 2025-05-xx second-level timestamp
        let ms = ApiTransDomain::normalize_chain_time_to_ms(sec).unwrap();
        assert_eq!(ms, sec * 1000);
    }

    #[test]
    fn normalize_chain_time_milliseconds_kept_as_is() {
        let ms = 1_746_040_371_123_u128;
        let normalized = ApiTransDomain::normalize_chain_time_to_ms(ms).unwrap();
        assert_eq!(normalized, ms);
    }

    #[test]
    fn normalize_chain_time_zero_should_fail() {
        assert!(ApiTransDomain::normalize_chain_time_to_ms(0).is_err());
    }
}

#[cfg(test)]
mod recover_tx_status_tests {
    use super::ApiTransDomain;

    #[test]
    fn recovered_tx_status_two_is_success() {
        assert!(ApiTransDomain::is_recovered_tx_status_success("0xtx", 2));
    }

    #[test]
    fn recovered_tx_status_three_is_failure() {
        assert!(!ApiTransDomain::is_recovered_tx_status_success("0xtx", 3));
    }

    #[test]
    fn recovered_tx_unknown_status_is_failure() {
        assert!(!ApiTransDomain::is_recovered_tx_status_success("0xtx", 1));
    }

    #[test]
    fn recovered_transfer_resp_uses_local_fee_and_chain_time() {
        let resp = ApiTransDomain::recovered_transfer_resp("0xtx", "1.23", 1_746_040_371_123);

        assert_eq!(resp.tx_hash, "0xtx");
        assert_eq!(resp.fee, "1.23");
        assert_eq!(resp.transaction_time_ms, Some(1_746_040_371_123));
        assert!(resp.consumer.is_none());
    }
}

#[cfg(test)]
mod nonce_broadcast_conflict_tests {
    use super::ApiTransDomain;
    use crate::error::service::ServiceError;

    #[test]
    fn evm_nonce_too_low_with_local_tx_state_should_be_treated_as_broadcast_success() {
        let err = ServiceError::Parameter(
            "node response: Node response error: code=-32000, rpc=https://example, message=Some(\"nonce too low: next nonce 3, tx nonce 2\")".to_string(),
        );

        assert!(ApiTransDomain::should_treat_nonce_too_low_as_broadcast_success(
            "eth", &err, true, true,
        ));
    }

    #[test]
    fn nonce_too_low_without_local_tx_state_should_not_be_treated_as_broadcast_success() {
        let err = ServiceError::Parameter(
            "node response: Node response error: code=-32000, rpc=https://example, message=Some(\"nonce too low: next nonce 3, tx nonce 2\")".to_string(),
        );

        assert!(!ApiTransDomain::should_treat_nonce_too_low_as_broadcast_success(
            "eth", &err, false, true,
        ));
        assert!(!ApiTransDomain::should_treat_nonce_too_low_as_broadcast_success(
            "sol", &err, true, true,
        ));
    }
}

pub(crate) struct ApiTransDomain {}

impl ApiTransDomain {
    fn is_evm_chain(chain_code: &str) -> bool {
        chain_code == ChainCode::Ethereum.to_string()
            || chain_code == ChainCode::BnbSmartChain.to_string()
    }

    fn is_sol_chain(chain_code: &str) -> bool {
        chain_code == ChainCode::Solana.to_string()
    }

    fn need_broadcast_visibility_check(chain_code: &str) -> bool {
        Self::is_evm_chain(chain_code) || Self::is_sol_chain(chain_code)
    }

    fn warn_chain_rpc_failure(
        error: &impl Display,
        log: chain_rpc_guard::ChainRpcFailureLog<'_>,
        reason: &str,
    ) -> chain_rpc_guard::ChainRpcErrorContext {
        let rpc_error = chain_rpc_guard::ChainRpcErrorContext::from_display(error);
        log.warn(&rpc_error, reason);
        rpc_error.record_transient_failure();
        rpc_error
    }

    fn error_chain_rpc_failure(
        error: &impl Display,
        log: chain_rpc_guard::ChainRpcFailureLog<'_>,
        reason: &str,
    ) -> chain_rpc_guard::ChainRpcErrorContext {
        let rpc_error = chain_rpc_guard::ChainRpcErrorContext::from_display(error);
        log.error(&rpc_error, reason);
        rpc_error.record_transient_failure();
        rpc_error
    }

    fn log_visibility_probe(
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

    fn evm_raw_hash_hint(raw: &[u8]) -> String {
        let digest = Keccak256::digest(raw);
        format!("0x{}", hex::encode(digest))
    }

    fn normalize_expected_tx_hash(expected_tx_hash: Option<&str>) -> Option<String> {
        expected_tx_hash.map(str::trim).filter(|s| !s.is_empty()).map(ToOwned::to_owned)
    }

    fn select_broadcast_tx_hash_hint(
        raw: &RawTx,
        expected_tx_hash: Option<&str>,
    ) -> Option<String> {
        match raw {
            RawTx::Tron(raw, ..) => Some(raw.tx_id.clone()),
            RawTx::Evm(raw, ..) => Some(Self::evm_raw_hash_hint(raw)),
            // SOL duplicate 场景必须使用构建阶段持久化的 tx_hash；
            // RawTx::Sol 第一字段是序列化串，不是可查询签名。
            RawTx::Sol(..) => Self::normalize_expected_tx_hash(expected_tx_hash),
        }
    }

    fn resolve_duplicate_tx_hash(
        expected_tx_hash: Option<&str>,
        fallback_hint: Option<&str>,
    ) -> Option<String> {
        Self::normalize_expected_tx_hash(expected_tx_hash)
            .or_else(|| fallback_hint.map(ToOwned::to_owned))
    }

    fn normalize_chain_time_to_ms(raw: u128) -> Result<u128, ServiceError> {
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
                "chain transaction_time out of range for DateTime::from_timestamp_millis"
                    .to_string(),
            )));
        }

        Ok(normalized)
    }

    fn clone_raw_for_single_retry(raw: &RawTx) -> Option<RawTx> {
        match raw {
            RawTx::Evm(raw, fee) => Some(RawTx::Evm(raw.clone(), *fee)),
            RawTx::Sol(sig, serialized) => Some(RawTx::Sol(sig.clone(), serialized.clone())),
            RawTx::Tron(..) => None,
        }
    }

    pub(crate) fn is_duplicate_broadcast_error(err: &ServiceError) -> bool {
        let s = err.to_string().to_ascii_lowercase();
        // Tron (nileex) observed:
        // - "rpc error Transaction already exists."
        // - "rpc error Dup transaction."
        //
        // EVM common patterns:
        // - "known transaction"
        // - "already known"
        //
        // Solana observed patterns:
        // - "Transaction simulation failed: This transaction has already been processed"
        // - "Transaction simulation failed: This transn has already been processed"
        //
        // These indicate the tx is already accepted/seen by the node; treat broadcast as idempotent.
        (s.contains("transaction") && (s.contains("already exists") || s.contains("already known")))
            || s.contains("known transaction")
            || s.contains("dup transaction")
            || s.contains("duplicate transaction")
            || s.contains("already been processed")
            || s.contains("has already been processed")
    }

    pub(crate) fn is_evm_chain_code(chain_code: &str) -> bool {
        chain_code.eq_ignore_ascii_case("eth") || chain_code.eq_ignore_ascii_case("bnb")
    }

    pub(crate) fn is_nonce_too_low_error(err: &ServiceError) -> bool {
        let s = err.to_string().to_ascii_lowercase();
        s.contains("nonce too low")
            || s.contains("nonce is too low")
            || s.contains("invalid nonce: too low")
            || s.contains("nonce already used")
            || s.contains("nonce has already been used")
            || s.contains("invalid nonce: already used")
    }

    pub(crate) fn should_treat_nonce_too_low_as_broadcast_success(
        chain_code: &str,
        err: &ServiceError,
        has_raw_tx: bool,
        has_tx_hash: bool,
    ) -> bool {
        has_raw_tx
            && has_tx_hash
            && Self::is_evm_chain_code(chain_code)
            && Self::is_nonce_too_low_error(err)
    }

    pub(crate) fn is_blockhash_not_found_error(err: &ServiceError) -> bool {
        let s = err.to_string().to_ascii_lowercase();
        s.contains("blockhash not found") || s.contains("block hash not found")
    }

    pub(crate) fn is_transaction_expired_error(err: &ServiceError) -> bool {
        err.to_string().to_ascii_lowercase().contains("transaction expired")
    }

    pub(crate) fn should_invalidate_tron_expired_broadcast(
        chain_code: &str,
        err: &ServiceError,
        has_raw_tx: bool,
        has_tx_hash: bool,
        last_broadcast_at_present: bool,
    ) -> bool {
        chain_code.eq_ignore_ascii_case("tron")
            && Self::is_transaction_expired_error(err)
            && has_raw_tx
            && has_tx_hash
            && !last_broadcast_at_present
    }

    pub(crate) fn is_recoverable_sol_broadcast_error(_err: &ServiceError) -> bool {
        false
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

    // transfer（显式上下文签名）
    pub async fn transfer(
        ctx: &'static Context,
        params: ApiTransferReq,
        preloaded_private_key: Option<ChainPrivateKey>,
    ) -> Result<TransferResp, ServiceError> {
        Self::transfer_with_ctx(ctx, params, preloaded_private_key).await
    }

    /// transfer（显式上下文签名）
    pub async fn transfer_with_ctx(
        ctx: &'static Context,
        params: ApiTransferReq,
        preloaded_private_key: Option<ChainPrivateKey>,
    ) -> Result<TransferResp, ServiceError> {
        let start_time = Instant::now();
        tracing::info!(
            "transfer (开始): 请求ID: {:?}, 链: {}, 时间: {:?}",
            params.base.request_resource_id,
            params.base.chain_code,
            start_time
        );

        tracing::info!("transfer: 获取私钥");
        let private_key_time = Instant::now();
        let private_key = match preloaded_private_key {
            Some(pk) => pk,
            None => {
                ApiAccountDomain::get_private_key_with_ctx(
                    ctx,
                    &params.base.from,
                    &params.base.chain_code,
                )
                .await?
            }
        };
        tracing::info!("transfer: 获取私钥完成, 耗时: {:?}", private_key_time.elapsed());

        tracing::info!("transfer: 原始链代码: {}", params.base.chain_code);
        let chain_code_time = Instant::now();
        let chain_code = params.base.chain_code.as_str();
        tracing::info!(
            "transfer: 转换后链代码: {}, 耗时: {:?}",
            chain_code,
            chain_code_time.elapsed()
        );

        let adapter_time = Instant::now();
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter_with_ctx(ctx, chain_code).await?;
        tracing::info!("transfer (适配器创建): 完成, 耗时: {:?}", adapter_time.elapsed());

        tracing::info!("transfer: 执行转账");
        // TODO：可优化
        let transfer_time = Instant::now();
        let resp = adapter.transfer_with_ctx(ctx, &params, private_key).await?;
        tracing::info!("transfer: 转账操作完成, 耗时: {:?}", transfer_time.elapsed());

        if let Some(request_id) = params.base.request_resource_id {
            tracing::info!("transfer (委托完成): 开始, request_id: {}", request_id);
            let delegate_time = Instant::now();
            let backend = ctx.get_global_backend_api();
            let _ = backend.delegate_complete(&request_id).await;
            tracing::info!("transfer (委托完成): 结束, 耗时: {:?}", delegate_time.elapsed());
        }

        tracing::info!("transfer (结束): 总耗时: {:?}", start_time.elapsed());
        Ok(resp)
    }

    /// 构建原始交易
    ///
    /// 职责：
    /// - 只负责构建交易，生成raw_tx和tx_hash
    /// - 不处理Recover逻辑
    /// - 不负责从链上恢复交易状态
    pub async fn build_transfer_raw(
        ctx: &'static Context,
        params: ApiTransferReq,
        preloaded_private_key: Option<ChainPrivateKey>,
    ) -> Result<(String, RawTx, String), ServiceError> {
        Self::build_transfer_raw_with_ctx(ctx, params, preloaded_private_key).await
    }

    pub async fn build_transfer_raw_with_ctx(
        ctx: &'static Context,
        params: ApiTransferReq,
        preloaded_private_key: Option<ChainPrivateKey>,
    ) -> Result<(String, RawTx, String), ServiceError> {
        let start_time = Instant::now();
        tracing::info!(
            "transfer (开始): 请求ID: {:?}, 链: {}, 时间: {:?}",
            params.base.request_resource_id,
            params.base.chain_code,
            start_time
        );

        tracing::info!("transfer: 获取私钥");
        let private_key_time = Instant::now();
        let private_key = match preloaded_private_key {
            Some(pk) => pk,
            None => {
                ApiAccountDomain::get_private_key_with_ctx(
                    ctx,
                    &params.base.from,
                    &params.base.chain_code,
                )
                .await?
            }
        };
        tracing::info!("transfer: 获取私钥完成, 耗时: {:?}", private_key_time.elapsed());

        tracing::info!("transfer: 原始链代码: {}", params.base.chain_code);
        let chain_code_time = Instant::now();
        let chain_code = params.base.chain_code.as_str();
        tracing::info!(
            "transfer: 转换后链代码: {}, 耗时: {:?}",
            chain_code,
            chain_code_time.elapsed()
        );

        let adapter_time = Instant::now();
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter_with_ctx(ctx, chain_code).await?;
        tracing::info!("transfer (适配器创建): 完成, 耗时: {:?}", adapter_time.elapsed());

        tracing::info!("transfer: 执行转账");
        // TODO：可优化
        let transfer_time = Instant::now();
        tracing::info!(
            chain_code = %params.base.chain_code,
            from = %params.base.from,
            to = %params.base.to,
            value = %params.base.value,
            token_address = ?params.base.token_address,
            request_resource_id = ?params.base.request_resource_id,
            nonce = params.nonce,
            "transfer: 构建原始交易"
        );
        let resp = adapter.build_transfer_raw(&params, private_key).await?;
        tracing::info!("transfer: 转账操作完成, 耗时: {:?}", transfer_time.elapsed());
        tracing::info!(
            chain_code = %chain_code,
            request_resource_id = ?params.base.request_resource_id,
            "transfer: 构建原始交易完成"
        );

        Ok(resp)
    }

    /// ⚠️ 注意：
    /// 网络错误不等于广播失败。
    /// broadcast_transfer 在网络异常时返回 Ok(None)，
    /// 由 scanner / recover 决定最终状态。
    pub async fn broadcast_transfer(
        ctx: &'static Context,
        chain_code: &str,
        raw: RawTx,
        expected_tx_hash: Option<&str>,
    ) -> Result<Option<TransferResp>, ServiceError> {
        Self::broadcast_transfer_with_ctx(ctx, chain_code, raw, expected_tx_hash).await
    }

    /// ⚠️ 注意：
    /// 网络错误不等于广播失败。
    /// broadcast_transfer 在网络异常时返回 Ok(None)，
    /// 由 scanner / recover 决定最终状态。
    pub async fn broadcast_transfer_with_ctx(
        ctx: &'static Context,
        chain_code: &str,
        raw: RawTx,
        expected_tx_hash: Option<&str>,
    ) -> Result<Option<TransferResp>, ServiceError> {
        let start_time = Instant::now();
        tracing::info!("broadcast_transfer (开始): 链: {}, 时间: {:?}", chain_code, start_time);

        let tx_hash_hint = Self::select_broadcast_tx_hash_hint(&raw, expected_tx_hash);
        if let RawTx::Evm(raw, ..) = &raw {
            tracing::info!(
                chain_code = %chain_code,
                local_raw_hash = %Self::evm_raw_hash_hint(raw),
                raw_len = raw.len(),
                "broadcast_transfer prepared EVM raw tx hash hint"
            );
        }

        if let Some((host, remaining)) =
            chain_rpc_guard::breaker_open_for_chain_code_with_ctx(ctx, chain_code).await
        {
            if let Some(tx_hash) = tx_hash_hint.as_deref() {
                tracing::warn!(
                    chain_code = %chain_code,
                    host = %host,
                    remaining = ?remaining,
                    tx_hash = %tx_hash,
                    "chain rpc circuit breaker open; skip broadcast in this round"
                );
            } else {
                tracing::warn!(
                    chain_code = %chain_code,
                    host = %host,
                    remaining = ?remaining,
                    "chain rpc circuit breaker open; skip broadcast in this round"
                );
            }
            return Ok(None);
        }

        let adapter_time = Instant::now();
        let mut auth_retry_attempted = false;
        let mut first_raw_attempt = Some(raw);
        let mut retry_raw_once =
            first_raw_attempt.as_ref().and_then(Self::clone_raw_for_single_retry);
        'auth_retry: loop {
            let adapter =
                match ApiChainAdapterFactory::get_transaction_adapter_with_ctx(ctx, chain_code)
                    .await
                {
                    Ok(adapter) => adapter,
                    Err(e) if !auth_retry_attempted && e.is_rpc_auth_unauthorized() => {
                        auth_retry_attempted = true;
                        Self::refresh_rpc_auth_and_prepare_retry(
                            ctx,
                            chain_code,
                            "broadcast_transfer:get_adapter",
                            None,
                            &e,
                        )
                        .await?;
                        continue 'auth_retry;
                    }
                    Err(e) => {
                        if e.is_delay_retryable() {
                            tracing::warn!("broadcast_transfer: 延迟重试, 适配器创建失败: {}", e);
                            return Ok(None);
                        }
                        tracing::error!("broadcast_transfer: 非延迟重试, 适配器创建失败: {}", e);
                        return Err(e);
                    }
                };
            tracing::info!(
                "broadcast_transfer (适配器创建): 完成, 耗时: {:?}",
                adapter_time.elapsed()
            );
            let rpc = adapter.rpc_endpoint_for_log().unwrap_or_else(|| "<unknown>".to_string());

            let raw_for_broadcast = if let Some(raw_once) = first_raw_attempt.take() {
                raw_once
            } else if let Some(raw_retry) = retry_raw_once.take() {
                raw_retry
            } else {
                tracing::warn!(
                    chain_code = %chain_code,
                    rpc = %rpc,
                    "auth retry requested but raw tx is not cloneable for this chain; treat as uncertain"
                );
                return Ok(None);
            };

            let resp = match adapter.broadcast_transfer(raw_for_broadcast).await {
                Ok(resp) => resp,
                Err(e) if !auth_retry_attempted && e.is_rpc_auth_unauthorized() => {
                    auth_retry_attempted = true;
                    Self::refresh_rpc_auth_and_prepare_retry(
                        ctx,
                        chain_code,
                        "broadcast_transfer:broadcast",
                        Some(&rpc),
                        &e,
                    )
                    .await?;
                    continue 'auth_retry;
                }
                Err(e) => {
                    if Self::is_duplicate_broadcast_error(&e) {
                        let tx_hash = if let Some(tx_hash) = Self::resolve_duplicate_tx_hash(
                            expected_tx_hash,
                            tx_hash_hint.as_deref(),
                        ) {
                            tx_hash
                        } else {
                            tracing::warn!(
                                chain_code = %chain_code,
                                error = %e,
                                "broadcast duplicate/exists but missing tx_hash; treat as uncertain"
                            );
                            return Ok(None);
                        };
                        let synthetic = TransferResp::new(tx_hash, String::new());
                        if Self::need_broadcast_visibility_check(chain_code) {
                            tracing::warn!(
                                chain_code = %chain_code,
                                tx_hash = %synthetic.tx_hash,
                                error = %e,
                                "broadcast duplicate/exists; will verify same-rpc visibility before treating as success"
                            );
                        } else {
                            tracing::warn!(
                                chain_code = %chain_code,
                                tx_hash = %synthetic.tx_hash,
                                error = %e,
                                "broadcast duplicate/exists; treat as idempotent success"
                            );
                            return Ok(Some(synthetic));
                        }
                        synthetic
                    } else if chain_code.eq_ignore_ascii_case(&ChainCode::Tron.to_string())
                        && Self::is_transaction_expired_error(&e)
                    {
                        Self::warn_chain_rpc_failure(
                            &e,
                            chain_rpc_guard::ChainRpcFailureLog::new()
                                .chain_code(chain_code)
                                .rpc(&rpc)
                                .op("broadcast_transfer"),
                            "broadcast transaction expired; return error so raw tx can be rebuilt",
                        );
                        return Err(e);
                    } else if e.is_network_error() {
                        Self::error_chain_rpc_failure(
                            &e,
                            chain_rpc_guard::ChainRpcFailureLog::new()
                                .chain_code(chain_code)
                                .rpc(&rpc)
                                .op("broadcast_transfer"),
                            "broadcast_transfer: 网络错误, 交易广播失败",
                        );
                        if auth_retry_attempted {
                            tracing::warn!(chain_code=%chain_code, rpc=%rpc, op="broadcast_transfer", error=%e, "auth retry failed");
                        }
                        return Ok(None);
                    } else {
                        chain_rpc_guard::ChainRpcErrorContext::from_display(&e)
                            .record_transient_failure();
                        if auth_retry_attempted {
                            tracing::warn!(chain_code=%chain_code, rpc=%rpc, op="broadcast_transfer", error=%e, "auth retry failed");
                        }
                        return Err(e);
                    }
                }
            };
            tracing::info!("broadcast_transfer: 转账操作完成, 耗时: {:?}", start_time.elapsed());
            tracing::info!(
                chain_code = %chain_code,
                tx_hash = %resp.tx_hash,
                fee = %resp.fee,
                rpc = %rpc,
                "broadcast_transfer response received"
            );

            if Self::need_broadcast_visibility_check(chain_code) {
                let visibility_kind = if Self::is_evm_chain(chain_code) { "evm" } else { "sol" };
                let visibility_probe = match adapter.tx_visibility_probe(&resp.tx_hash).await {
                    Ok(probe) => probe,
                    Err(e) if !auth_retry_attempted && e.is_rpc_auth_unauthorized() => {
                        auth_retry_attempted = true;
                        Self::refresh_rpc_auth_and_prepare_retry(
                            ctx,
                            chain_code,
                            "broadcast_transfer:visibility_probe",
                            Some(&rpc),
                            &e,
                        )
                        .await?;
                        continue 'auth_retry;
                    }
                    Err(e) => {
                        if e.is_delay_retryable() {
                            Self::warn_chain_rpc_failure(
                                &e,
                                chain_rpc_guard::ChainRpcFailureLog::new()
                                    .chain_code(chain_code)
                                    .rpc(&rpc)
                                    .visibility_kind(visibility_kind)
                                    .op("broadcast_transfer:visibility_probe"),
                                "broadcast visibility probe failed with delay retryable error",
                            );
                            return Ok(None);
                        }
                        Self::warn_chain_rpc_failure(
                            &e,
                            chain_rpc_guard::ChainRpcFailureLog::new()
                                .chain_code(chain_code)
                                .rpc(&rpc)
                                .visibility_kind(visibility_kind)
                                .op("broadcast_transfer:visibility_probe"),
                            "broadcast visibility probe failed",
                        );
                        return Ok(None);
                    }
                };

                if let Some(probe) = &visibility_probe {
                    Self::log_visibility_probe(
                        chain_code,
                        &resp.tx_hash,
                        &rpc,
                        visibility_kind,
                        probe,
                        None,
                        None,
                        "start",
                    );
                } else {
                    tracing::info!(
                        chain_code = %chain_code,
                        tx_hash = %resp.tx_hash,
                        rpc = %rpc,
                        visibility_kind = %visibility_kind,
                        "broadcast visibility check start"
                    );
                }

                for (idx, delay_ms) in [200_u64, 500_u64, 1000_u64].iter().enumerate() {
                    sleep(std::time::Duration::from_millis(*delay_ms)).await;
                    let attempt = idx + 1;

                    if let Some(probe) = &visibility_probe {
                        if probe.seen_on_node {
                            Self::log_visibility_probe(
                                chain_code,
                                &resp.tx_hash,
                                &rpc,
                                visibility_kind,
                                probe,
                                Some(attempt),
                                Some(*delay_ms),
                                "hit",
                            );
                            chain_rpc_guard::record_success_for_chain_code_with_ctx(
                                ctx, chain_code,
                            )
                            .await;
                            if auth_retry_attempted {
                                tracing::info!(chain_code=%chain_code, rpc=%rpc, op="broadcast_transfer", "auth retry succeeded");
                            }
                            return Ok(Some(resp));
                        }

                        Self::log_visibility_probe(
                            chain_code,
                            &resp.tx_hash,
                            &rpc,
                            visibility_kind,
                            probe,
                            Some(attempt),
                            Some(*delay_ms),
                            "pending miss",
                        );
                    } else {
                        match adapter.query_tx_seen_on_node(&resp.tx_hash).await {
                            Ok(true) => {
                                tracing::info!(
                                    chain_code = %chain_code,
                                    tx_hash = %resp.tx_hash,
                                    rpc = %rpc,
                                    attempt = attempt,
                                    delay_ms = *delay_ms,
                                    visibility_kind = %visibility_kind,
                                    "broadcast visibility check hit"
                                );
                                chain_rpc_guard::record_success_for_chain_code_with_ctx(
                                    ctx, chain_code,
                                )
                                .await;
                                if auth_retry_attempted {
                                    tracing::info!(chain_code=%chain_code, rpc=%rpc, op="broadcast_transfer", "auth retry succeeded");
                                }
                                return Ok(Some(resp));
                            }
                            Ok(false) => {
                                tracing::info!(
                                    chain_code = %chain_code,
                                    tx_hash = %resp.tx_hash,
                                    rpc = %rpc,
                                    attempt = attempt,
                                    delay_ms = *delay_ms,
                                    visibility_kind = %visibility_kind,
                                    "broadcast visibility check pending miss"
                                );
                            }
                            Err(e) if !auth_retry_attempted && e.is_rpc_auth_unauthorized() => {
                                auth_retry_attempted = true;
                                Self::refresh_rpc_auth_and_prepare_retry(
                                    ctx,
                                    chain_code,
                                    "broadcast_transfer:visibility_check",
                                    Some(&rpc),
                                    &e,
                                )
                                .await?;
                                continue 'auth_retry;
                            }
                            Err(e) => {
                                Self::warn_chain_rpc_failure(
                                    &e,
                                    chain_rpc_guard::ChainRpcFailureLog::new()
                                        .chain_code(chain_code)
                                        .rpc(&rpc)
                                        .tx_hash(&resp.tx_hash)
                                        .visibility_kind(visibility_kind)
                                        .op("broadcast_transfer:visibility_check")
                                        .attempt(attempt as u32)
                                        .delay_ms(*delay_ms as u64),
                                    "broadcast visibility check miss (uncertain)",
                                );
                                if auth_retry_attempted {
                                    tracing::warn!(chain_code=%chain_code, rpc=%rpc, op="broadcast_transfer", error=%e, "auth retry failed");
                                }
                                return Ok(None);
                            }
                        }
                    }
                }

                tracing::warn!(
                    chain_code = %chain_code,
                    tx_hash = %resp.tx_hash,
                    rpc = %rpc,
                    visibility_kind = %visibility_kind,
                    "broadcast visibility check miss (uncertain)"
                );
                if auth_retry_attempted {
                    tracing::warn!(chain_code=%chain_code, rpc=%rpc, op="broadcast_transfer", "auth retry failed");
                }
                return Ok(None);
            }

            chain_rpc_guard::record_success_for_chain_code_with_ctx(ctx, chain_code).await;
            if auth_retry_attempted {
                tracing::info!(chain_code=%chain_code, rpc=%rpc, op="broadcast_transfer", "auth retry succeeded");
            }
            return Ok(Some(resp));
        }
    }

    pub async fn nonce(
        ctx: &'static Context,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<u64, ServiceError> {
        Self::nonce_with_ctx(ctx, from_addr, chain_code).await
    }

    pub async fn nonce_with_ctx(
        ctx: &'static Context,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<u64, ServiceError> {
        let start_time = Instant::now();
        tracing::info!(
            "nonce (开始): from_addr: {}, chain_code: {}, 时间: {:?}",
            from_addr,
            chain_code,
            start_time
        );

        let chain_code_time = Instant::now();
        tracing::info!("nonce (链代码转换): 完成, 耗时: {:?}", chain_code_time.elapsed());

        let adapter_time = Instant::now();
        let mut auth_retry_attempted = false;
        loop {
            let adapter =
                match ApiChainAdapterFactory::get_transaction_adapter_with_ctx(ctx, chain_code)
                    .await
                {
                    Ok(adapter) => adapter,
                    Err(e) if !auth_retry_attempted && e.is_rpc_auth_unauthorized() => {
                        auth_retry_attempted = true;
                        Self::refresh_rpc_auth_and_prepare_retry(
                            ctx,
                            chain_code,
                            "nonce:get_adapter",
                            None,
                            &e,
                        )
                        .await?;
                        continue;
                    }
                    Err(e) => {
                        tracing::info!("nonce (结束): 总耗时: {:?}", start_time.elapsed());
                        return Err(e);
                    }
                };
            tracing::info!("nonce (适配器创建): 完成, 耗时: {:?}", adapter_time.elapsed());
            let rpc = adapter.rpc_endpoint_for_log().unwrap_or_else(|| "<unknown>".to_string());

            match adapter.nonce(from_addr).await {
                Ok(resp) => {
                    tracing::info!("nonce (结束): 总耗时: {:?}", start_time.elapsed());
                    if auth_retry_attempted {
                        tracing::info!(chain_code=%chain_code, rpc=%rpc, op="nonce", "auth retry succeeded");
                    }
                    return Ok(resp);
                }
                Err(e) if !auth_retry_attempted && e.is_rpc_auth_unauthorized() => {
                    auth_retry_attempted = true;
                    Self::refresh_rpc_auth_and_prepare_retry(
                        ctx,
                        chain_code,
                        "nonce:rpc_call",
                        Some(&rpc),
                        &e,
                    )
                    .await?;
                    continue;
                }
                Err(e) => {
                    tracing::info!("nonce (结束): 总耗时: {:?}", start_time.elapsed());
                    if auth_retry_attempted {
                        tracing::warn!(chain_code=%chain_code, rpc=%rpc, op="nonce", error=%e, "auth retry failed");
                    }
                    return Err(e);
                }
            }
        }
    }

    /// 处理已生成raw_tx的交易恢复逻辑
    ///
    /// 职责：
    /// - 只负责从链上恢复交易状态
    /// - 不负责构建交易
    /// - 使用链上时间设置 transaction_time_ms
    ///
    /// 显式不变量：
    /// - transaction_time MUST come from on-chain confirmation (chain timestamp)
    /// - last_broadcast_at MUST be backfilled with the same value as transaction_time
    /// - This ensures both fields reflect the same chain-based timestamp
    ///
    /// 返回语义约定：
    /// Ok(Some(resp))  -> 链上已确认成功，立即落成
    /// Err(_)          -> 已确认不可能成功，可推进失败
    /// Ok(None)        -> 不确定态（RPC/网络/索引问题），等待 scanner/recover
    pub async fn process_recovered_tx(
        ctx: &'static Context,
        chain_code: &str,
        from_addr: &str,
        tx_hash: &str,
        // raw_tx: &str,
        nonce: i64,
        transaction_fee: &str,
    ) -> Result<Option<TransferResp>, ServiceError> {
        Self::process_recovered_tx_with_ctx(
            &ctx,
            chain_code,
            from_addr,
            tx_hash,
            nonce,
            transaction_fee,
        )
        .await
    }

    pub async fn process_recovered_tx_with_ctx(
        ctx: &'static Context,
        chain_code: &str,
        from_addr: &str,
        tx_hash: &str,
        // raw_tx: &str,
        nonce: i64,
        transaction_fee: &str,
    ) -> Result<Option<TransferResp>, ServiceError> {
        tracing::info!(tx_hash=?tx_hash, "检测到已有raw_tx和tx_hash，执行恢复检查");

        if let Some(detail) =
            chain_rpc_guard::breaker_open_detail_for_chain_code_with_ctx(ctx, chain_code).await
        {
            tracing::warn!(
                reason = "rpc_circuit_breaker_open",
                chain_code = %chain_code,
                host = %detail.host,
                remaining = ?detail.remaining,
                last_error_code = ?detail.last_error_code,
                tx_hash = %tx_hash,
                "Recover query skipped because chain RPC circuit breaker is open"
            );
            return Ok(None);
        }

        let mut auth_retry = RecoverAuthRetryState::default();
        'recover_auth_retry: loop {
            let adapter =
                match Self::acquire_recover_adapter(ctx, &mut auth_retry, chain_code).await? {
                    RecoverAdapterAcquire::Ready(adapter) => adapter,
                    RecoverAdapterAcquire::RetryAuth => continue 'recover_auth_retry,
                    RecoverAdapterAcquire::StayUncertain => return Ok(None),
                };
            let rpc = adapter.rpc_endpoint_for_log().unwrap_or_else(|| "<unknown>".to_string());

            match recover_query_outcome(adapter.query_tx_res(tx_hash).await) {
                // === A. 链上查到了 ===
                RecoverQueryOutcome::Found(tx_result) => {
                    if auth_retry.attempted() {
                        tracing::info!(chain_code=%chain_code, rpc=%rpc, op=RECOVER_OP_QUERY_TX_RES, "auth retry succeeded");
                    }
                    return Self::handle_recovered_tx_result(
                        ctx,
                        chain_code,
                        tx_hash,
                        transaction_fee,
                        tx_result,
                    )
                    .await;
                }

                // === B. 链上没有该hash ===
                RecoverQueryOutcome::Missing => {
                    return Self::handle_missing_recovered_tx(
                        ctx,
                        chain_code,
                        from_addr,
                        tx_hash,
                        nonce,
                        &rpc,
                        adapter.as_ref(),
                    )
                    .await;
                }

                // === C. RPC 异常 ===
                RecoverQueryOutcome::QueryError(service_err) => {
                    match Self::handle_recover_query_error_action(
                        ctx,
                        &mut auth_retry,
                        chain_code,
                        tx_hash,
                        &rpc,
                        &service_err,
                    )
                    .await?
                    {
                        RecoverQueryErrorAction::RetryAuth => continue 'recover_auth_retry,
                        RecoverQueryErrorAction::StayUncertain => return Ok(None),
                    }
                }
            }
        }
    }

    async fn acquire_recover_adapter(
        ctx: &'static Context,
        auth_retry: &mut RecoverAuthRetryState,
        chain_code: &str,
    ) -> Result<RecoverAdapterAcquire, ServiceError> {
        match ApiChainAdapterFactory::get_transaction_adapter_with_ctx(ctx, chain_code).await {
            Ok(adapter) => Ok(RecoverAdapterAcquire::Ready(adapter)),
            Err(e) => match auth_retry.classify_adapter_error(&e) {
                RecoverAdapterErrorDecision::RefreshAuthAndRetry => {
                    Self::prepare_recover_auth_retry(
                        ctx,
                        auth_retry,
                        chain_code,
                        RECOVER_OP_GET_ADAPTER,
                        None,
                        &e,
                    )
                    .await?;
                    Ok(RecoverAdapterAcquire::RetryAuth)
                }
                RecoverAdapterErrorDecision::StayUncertain => {
                    tracing::warn!("process_recovered_tx: 延迟重试, 适配器创建失败: {}", e);
                    Ok(RecoverAdapterAcquire::StayUncertain)
                }
                RecoverAdapterErrorDecision::Fatal => {
                    tracing::error!("process_recovered_tx: 非延迟重试, 适配器创建失败: {}", e);
                    Err(e)
                }
            },
        }
    }

    async fn prepare_recover_auth_retry(
        ctx: &'static Context,
        auth_retry: &mut RecoverAuthRetryState,
        chain_code: &str,
        op: &str,
        rpc: Option<&str>,
        err: &ServiceError,
    ) -> Result<(), ServiceError> {
        auth_retry.mark_attempted();
        Self::refresh_rpc_auth_and_prepare_retry(ctx, chain_code, op, rpc, err).await
    }

    async fn handle_recover_query_error_action(
        ctx: &'static Context,
        auth_retry: &mut RecoverAuthRetryState,
        chain_code: &str,
        tx_hash: &str,
        rpc: &str,
        service_err: &ServiceError,
    ) -> Result<RecoverQueryErrorAction, ServiceError> {
        match auth_retry.classify_query_error(service_err) {
            RecoverQueryErrorDecision::RefreshAuthAndRetry => {
                Self::prepare_recover_auth_retry(
                    ctx,
                    auth_retry,
                    chain_code,
                    RECOVER_OP_QUERY_TX_RES,
                    Some(rpc),
                    service_err,
                )
                .await?;
                Ok(RecoverQueryErrorAction::RetryAuth)
            }
            RecoverQueryErrorDecision::StayUncertain => {
                log_recover_query_error(
                    chain_code,
                    tx_hash,
                    rpc,
                    service_err,
                    auth_retry.attempted(),
                );
                Ok(RecoverQueryErrorAction::StayUncertain)
            }
        }
    }

    async fn handle_recovered_tx_result(
        ctx: &'static Context,
        chain_code: &str,
        tx_hash: &str,
        transaction_fee: &str,
        tx_result: wallet_chain_interact::QueryTransactionResult,
    ) -> Result<Option<TransferResp>, ServiceError> {
        let is_success = Self::is_recovered_tx_status_success(tx_hash, tx_result.status);
        let time_ms = Self::normalize_chain_time_to_ms(tx_result.transaction_time)?;

        if is_success {
            tracing::info!(trade_no=?tx_hash, "链上确认成功，直接落成");
            chain_rpc_guard::record_success_for_chain_code_with_ctx(ctx, chain_code).await;
            Ok(Some(Self::recovered_transfer_resp(tx_hash, transaction_fee, time_ms)))
        } else {
            tracing::warn!(trade_no=?tx_hash, "链上失败，直接标记失败");
            Err(ServiceError::System(crate::error::system::SystemError::Internal(
                "broadcasted tx failed".into(),
            )))
        }
    }

    fn recovered_transfer_resp(
        tx_hash: &str,
        transaction_fee: &str,
        transaction_time_ms: u128,
    ) -> TransferResp {
        let mut resp = TransferResp {
            tx_hash: tx_hash.to_string(),
            fee: transaction_fee.to_string(),
            consumer: None,
            transaction_time_ms: None,
        };
        resp.with_transaction_time(transaction_time_ms);
        resp
    }

    fn is_recovered_tx_status_success(tx_hash: &str, status: i8) -> bool {
        match status {
            2 => true,
            3 => false,
            _ => {
                tracing::warn!(trade_no=?tx_hash, "链上交易状态异常: {}", status);
                false
            }
        }
    }

    async fn handle_missing_recovered_tx(
        ctx: &'static Context,
        chain_code: &str,
        from_addr: &str,
        tx_hash: &str,
        nonce: i64,
        rpc: &str,
        adapter: &(dyn Tx + Sync),
    ) -> Result<Option<TransferResp>, ServiceError> {
        tracing::info!(trade_no=?tx_hash, "链上未找到该交易，执行恢复判定");

        match classify_recover_missing_tx_route(chain_code) {
            RecoverMissingTxRoute::SolanaVisibilityProbe => {
                Self::probe_sol_visibility_during_recover(chain_code, tx_hash, rpc, adapter).await;
                Self::log_recover_missing_tx_wait(
                    tx_hash,
                    RecoverMissingTxRoute::SolanaVisibilityProbe,
                );
                Ok(None)
            }
            RecoverMissingTxRoute::TronPendingPool => {
                Self::handle_tron_missing_recovered_tx(chain_code, tx_hash, rpc, adapter).await
            }
            RecoverMissingTxRoute::EvmNonceCheck => {
                Self::handle_evm_missing_recovered_tx(ctx, chain_code, from_addr, tx_hash, nonce)
                    .await
            }
            RecoverMissingTxRoute::WaitForScanner => {
                Self::log_recover_missing_tx_wait(tx_hash, RecoverMissingTxRoute::WaitForScanner);
                Ok(None)
            }
        }
    }

    fn log_recover_missing_tx_wait(tx_hash: &str, route: RecoverMissingTxRoute) {
        if let Some(reason) = route.wait_reason() {
            tracing::info!(trade_no=?tx_hash, "{}", reason);
        }
    }

    async fn probe_sol_visibility_during_recover(
        chain_code: &str,
        tx_hash: &str,
        rpc: &str,
        adapter: &(dyn Tx + Sync),
    ) {
        match adapter.tx_visibility_probe(tx_hash).await {
            Ok(Some(probe)) => {
                Self::log_visibility_probe(
                    chain_code, tx_hash, rpc, "sol", &probe, None, None, "recover",
                );
            }
            Ok(None) => {
                tracing::info!(
                    trade_no = ?tx_hash,
                    rpc = %rpc,
                    "Solana visibility probe unavailable during recover"
                );
            }
            Err(e) => {
                log_sol_visibility_probe_error(chain_code, tx_hash, rpc, &e);
            }
        }
    }

    async fn handle_tron_missing_recovered_tx(
        chain_code: &str,
        tx_hash: &str,
        rpc: &str,
        adapter: &(dyn Tx + Sync),
    ) -> Result<Option<TransferResp>, ServiceError> {
        match adapter.has_pending_tx(tx_hash).await {
            Ok(true) => {
                tracing::info!(
                    trade_no=?tx_hash,
                    "Tron pending pool still sees tx hash; keep observing"
                );
                Ok(None)
            }
            Ok(false) => {
                tracing::warn!(
                    trade_no=?tx_hash,
                    "Tron tx missing from both confirmed and pending pools"
                );
                Err(ServiceError::System(crate::error::system::SystemError::Internal(
                    "tron tx missing from confirmed and pending pools".into(),
                )))
            }
            Err(err) => {
                log_tron_pending_pool_query_error(chain_code, tx_hash, rpc, &err);
                Ok(None)
            }
        }
    }

    async fn handle_evm_missing_recovered_tx(
        ctx: &'static Context,
        chain_code: &str,
        from_addr: &str,
        tx_hash: &str,
        nonce: i64,
    ) -> Result<Option<TransferResp>, ServiceError> {
        let chain_nonce = match Self::nonce_with_ctx(ctx, from_addr, chain_code).await {
            Ok(chain_nonce) => chain_nonce,
            Err(e) => {
                if e.is_network_error() {
                    tracing::warn!(trade_no=?tx_hash, "恢复判定获取链上nonce失败(网络瞬时)，等待下一轮: {}", e);
                    return Ok(None);
                }
                tracing::error!(trade_no=?tx_hash, "恢复判定获取链上nonce失败: {}", e);
                return Err(e);
            }
        };

        let local_nonce = nonce as u64;
        tracing::warn!(
            trade_no=?tx_hash,
            chain_nonce = chain_nonce,
            local_nonce = local_nonce,
            "EVM recover: tx hash missing on-chain, comparing nonce state"
        );

        log_evm_missing_tx_hash_state(tx_hash, chain_nonce, local_nonce);
        Ok(None)
    }
}
