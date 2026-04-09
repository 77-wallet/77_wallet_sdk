use crate::{
    domain::{
        api_wallet::{
            account::ApiAccountDomain, adapter::tx::RawTx, adapter_factory::ApiChainAdapterFactory,
        },
        chain::TransferResp,
    },
    error::service::ServiceError,
    infrastructure::chain_rpc_guard,
    request::api_wallet::trans::ApiTransferReq,
};
use sha3::{Digest, Keccak256};
use std::time::Instant;
use tokio::time::sleep;
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_types::chain::chain::ChainCode;
use wallet_utils::RetryableError as _;

pub(crate) mod collect;
pub(crate) mod fee;
pub(crate) mod withdraw;

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
    fn rent_shortage_should_not_be_recoverable_simulation_failure() {
        let err = ServiceError::Parameter(
            "node response: Transaction simulation failed: Transaction results in an account (0) with insufficient funds for rent"
                .to_string(),
        );
        assert!(!ApiTransDomain::is_recoverable_sol_broadcast_error(&err));
    }

    #[test]
    fn generic_sol_simulation_failure_should_be_recoverable() {
        let err = ServiceError::Parameter(
            "node response: Transaction simulation failed: custom program error: 0x1".to_string(),
        );
        assert!(!ApiTransDomain::is_duplicate_broadcast_error(&err));
        assert!(!ApiTransDomain::is_blockhash_not_found_error(&err));
        assert!(ApiTransDomain::is_recoverable_sol_broadcast_error(&err));
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
mod evm_recover_nonce_state_tests {
    use super::{ApiTransDomain, EvmRecoverMissingTxHashState};

    #[test]
    fn evm_recover_missing_tx_hash_treats_nonce_consumed_as_uncertain() {
        let state = ApiTransDomain::classify_evm_missing_tx_hash_state(4, 3);
        assert_eq!(state, EvmRecoverMissingTxHashState::ChainNonceConsumedLocalHashMissing);
    }

    #[test]
    fn evm_recover_missing_tx_hash_detects_equal_nonce_as_uncertain() {
        let state = ApiTransDomain::classify_evm_missing_tx_hash_state(3, 3);
        assert_eq!(state, EvmRecoverMissingTxHashState::ChainNonceMatchesLocalNonce);
    }

    #[test]
    fn evm_recover_missing_tx_hash_detects_local_nonce_ahead() {
        let state = ApiTransDomain::classify_evm_missing_tx_hash_state(2, 3);
        assert_eq!(state, EvmRecoverMissingTxHashState::LocalNonceAheadOfChainNonce);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvmRecoverMissingTxHashState {
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

impl ApiTransDomain {
    fn is_evm_chain(chain_code: &str) -> bool {
        chain_code == ChainCode::Ethereum.to_string()
            || chain_code == ChainCode::BnbSmartChain.to_string()
    }

    fn classify_evm_missing_tx_hash_state(
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

    fn is_sol_chain(chain_code: &str) -> bool {
        chain_code == ChainCode::Solana.to_string()
    }

    fn need_broadcast_visibility_check(chain_code: &str) -> bool {
        Self::is_evm_chain(chain_code) || Self::is_sol_chain(chain_code)
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

    pub(crate) fn is_recoverable_sol_broadcast_error(err: &ServiceError) -> bool {
        let s = err.to_string().to_ascii_lowercase();
        // Solana 节点常见返回：
        // - "Transaction simulation failed: ..."
        // 对于非 duplicate / 非 blockhash / 非 rent shortage 的 simulation failure，
        // 优先作为不确定态处理，交给后续 scanner/recover 判定，避免误判硬失败。
        let is_simulation_failure =
            s.contains("transaction simulation failed") || s.contains("simulation failed");
        is_simulation_failure
            && !Self::is_duplicate_broadcast_error(err)
            && !Self::is_blockhash_not_found_error(err)
            && !s.contains("insufficient funds for rent")
            && !s.contains("rent-exempt reserve")
    }

    async fn refresh_rpc_auth_and_prepare_retry(
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

        let ctx = crate::context::get_context()?;
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

    /// transfer
    pub async fn transfer(
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
                ApiAccountDomain::get_private_key(&params.base.from, &params.base.chain_code)
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
        let adapter = ApiChainAdapterFactory::get_transaction_adapter(chain_code).await?;
        tracing::info!("transfer (适配器创建): 完成, 耗时: {:?}", adapter_time.elapsed());

        tracing::info!("transfer: 执行转账");
        // TODO：可优化
        let transfer_time = Instant::now();
        let resp = adapter.transfer(&params, private_key).await?;
        tracing::info!("transfer: 转账操作完成, 耗时: {:?}", transfer_time.elapsed());

        if let Some(request_id) = params.base.request_resource_id {
            tracing::info!("transfer (委托完成): 开始, request_id: {}", request_id);
            let delegate_time = Instant::now();
            let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
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
                ApiAccountDomain::get_private_key(&params.base.from, &params.base.chain_code)
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
        let adapter = ApiChainAdapterFactory::get_transaction_adapter(chain_code).await?;
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
            chain_rpc_guard::breaker_open_for_chain_code(chain_code).await
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
            let adapter = match ApiChainAdapterFactory::get_transaction_adapter(chain_code).await {
                Ok(adapter) => adapter,
                Err(e) if !auth_retry_attempted && e.is_rpc_auth_unauthorized() => {
                    auth_retry_attempted = true;
                    Self::refresh_rpc_auth_and_prepare_retry(
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
                    } else if Self::is_sol_chain(chain_code)
                        && Self::is_recoverable_sol_broadcast_error(&e)
                    {
                        tracing::warn!(
                            chain_code = %chain_code,
                            rpc = %rpc,
                            error = %e,
                            "broadcast_transfer: recoverable sol simulation failure, treat as uncertain"
                        );
                        return Ok(None);
                    } else if e.is_network_error() {
                        tracing::error!("broadcast_transfer: 网络错误, 交易广播失败: {}", e);
                        chain_rpc_guard::record_transient_failure_from_error(&e);
                        if auth_retry_attempted {
                            tracing::warn!(chain_code=%chain_code, rpc=%rpc, op="broadcast_transfer", error=%e, "auth retry failed");
                        }
                        return Ok(None);
                    } else {
                        chain_rpc_guard::record_transient_failure_from_error(&e);
                        if auth_retry_attempted {
                            tracing::warn!(chain_code=%chain_code, rpc=%rpc, op="broadcast_transfer", error=%e, "auth retry failed");
                        }
                        return Err(e);
                    }
                }
            };
            tracing::info!("broadcast_transfer: 转账操作完成, 耗时: {:?}", start_time.elapsed());

            if Self::need_broadcast_visibility_check(chain_code) {
                let visibility_kind = if Self::is_evm_chain(chain_code) { "evm" } else { "sol" };
                tracing::info!(
                    chain_code = %chain_code,
                    tx_hash = %resp.tx_hash,
                    rpc = %rpc,
                    visibility_kind = %visibility_kind,
                    "broadcast visibility check start"
                );

                for (idx, delay_ms) in [200_u64, 500_u64, 1000_u64].iter().enumerate() {
                    sleep(std::time::Duration::from_millis(*delay_ms)).await;
                    let attempt = idx + 1;

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
                            chain_rpc_guard::record_success_for_chain_code(chain_code).await;
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
                                chain_code,
                                "broadcast_transfer:visibility_check",
                                Some(&rpc),
                                &e,
                            )
                            .await?;
                            continue 'auth_retry;
                        }
                        Err(e) => {
                            tracing::warn!(
                                chain_code = %chain_code,
                                tx_hash = %resp.tx_hash,
                                rpc = %rpc,
                                attempt = attempt,
                                delay_ms = *delay_ms,
                                error = %e,
                                visibility_kind = %visibility_kind,
                                "broadcast visibility check miss (uncertain)"
                            );
                            chain_rpc_guard::record_transient_failure_from_error(&e);
                            if auth_retry_attempted {
                                tracing::warn!(chain_code=%chain_code, rpc=%rpc, op="broadcast_transfer", error=%e, "auth retry failed");
                            }
                            return Ok(None);
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

            chain_rpc_guard::record_success_for_chain_code(chain_code).await;
            if auth_retry_attempted {
                tracing::info!(chain_code=%chain_code, rpc=%rpc, op="broadcast_transfer", "auth retry succeeded");
            }
            return Ok(Some(resp));
        }
    }

    pub async fn nonce(from_addr: &str, chain_code: &str) -> Result<u64, ServiceError> {
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
            let adapter = match ApiChainAdapterFactory::get_transaction_adapter(chain_code).await {
                Ok(adapter) => adapter,
                Err(e) if !auth_retry_attempted && e.is_rpc_auth_unauthorized() => {
                    auth_retry_attempted = true;
                    Self::refresh_rpc_auth_and_prepare_retry(
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
        chain_code: &str,
        from_addr: &str,
        tx_hash: &str,
        // raw_tx: &str,
        nonce: i64,
        transaction_fee: &str,
    ) -> Result<Option<TransferResp>, ServiceError> {
        tracing::info!(trade_no=?tx_hash, "检测到已有raw_tx和tx_hash，执行恢复检查");

        if let Some((host, remaining)) =
            chain_rpc_guard::breaker_open_for_chain_code(chain_code).await
        {
            tracing::warn!(
                chain_code = %chain_code,
                host = %host,
                remaining = ?remaining,
                tx_hash = %tx_hash,
                "chain rpc circuit breaker open; skip recover query in this round"
            );
            return Ok(None);
        }

        let mut auth_retry_attempted = false;
        'recover_auth_retry: loop {
            let adapter = match ApiChainAdapterFactory::get_transaction_adapter(chain_code).await {
                Ok(adapter) => adapter,
                Err(e) if !auth_retry_attempted && e.is_rpc_auth_unauthorized() => {
                    auth_retry_attempted = true;
                    Self::refresh_rpc_auth_and_prepare_retry(
                        chain_code,
                        "process_recovered_tx:get_adapter",
                        None,
                        &e,
                    )
                    .await?;
                    continue 'recover_auth_retry;
                }
                Err(e) => {
                    if e.is_delay_retryable() {
                        tracing::warn!("process_recovered_tx: 延迟重试, 适配器创建失败: {}", e);
                        return Ok(None);
                    } else {
                        tracing::error!("process_recovered_tx: 非延迟重试, 适配器创建失败: {}", e);
                        return Err(e);
                    }
                }
            };
            let rpc = adapter.rpc_endpoint_for_log().unwrap_or_else(|| "<unknown>".to_string());

            // 1. 查链上是否存在
            match adapter.query_tx_res(tx_hash).await {
                // === A. 链上查到了 ===
                Ok(Some(tx_result)) => {
                    if auth_retry_attempted {
                        tracing::info!(chain_code=%chain_code, rpc=%rpc, op="process_recovered_tx:query_tx_res", "auth retry succeeded");
                    }
                    // 修复状态类型问题：将i8转换为bool
                    let is_success = match tx_result.status {
                        2 => true,
                        3 => false,
                        _ => {
                            tracing::warn!(trade_no=?tx_hash, "链上交易状态异常: {}", tx_result.status);
                            false
                        }
                    };

                    let time_ms = Self::normalize_chain_time_to_ms(tx_result.transaction_time)?;

                    if is_success {
                        tracing::info!(trade_no=?tx_hash, "链上确认成功，直接落成");
                        chain_rpc_guard::record_success_for_chain_code(chain_code).await;
                        // 直接标记成功
                        let mut mock_resp = TransferResp {
                            tx_hash: tx_hash.to_string(),
                            fee: transaction_fee.to_string(),
                            consumer: None,
                            transaction_time_ms: None,
                        };
                        // 使用链上时间设置 transaction_time_ms
                        mock_resp.with_transaction_time(time_ms);
                        return Ok(Some(mock_resp));
                    } else {
                        tracing::warn!(trade_no=?tx_hash, "链上失败，直接标记失败");
                        return Err(ServiceError::System(
                            crate::error::system::SystemError::Internal(
                                "broadcasted tx failed".into(),
                            ),
                        ));
                    }
                }

                // === B. 链上没有该hash ===
                Ok(None) => {
                    tracing::info!(trade_no=?tx_hash, "链上未找到该交易，执行恢复判定");

                    if chain_code == ChainCode::Tron.to_string() {
                        match adapter.has_pending_tx(tx_hash).await {
                            Ok(true) => {
                                tracing::info!(
                                    trade_no=?tx_hash,
                                    "Tron pending pool still sees tx hash; keep observing"
                                );
                                return Ok(None);
                            }
                            Ok(false) => {
                                tracing::warn!(
                                    trade_no=?tx_hash,
                                    "Tron tx missing from both confirmed and pending pools"
                                );
                                return Err(ServiceError::System(
                                    crate::error::system::SystemError::Internal(
                                        "tron tx missing from confirmed and pending pools".into(),
                                    ),
                                ));
                            }
                            Err(err) => {
                                tracing::warn!(
                                    trade_no=?tx_hash,
                                    error=%err,
                                    "Tron pending pool query failed; keep uncertain"
                                );
                                return Ok(None);
                            }
                        }
                    }

                    let is_evm_chain = chain_code
                        == wallet_types::chain::chain::ChainCode::Ethereum.to_string()
                        || chain_code
                            == wallet_types::chain::chain::ChainCode::BnbSmartChain.to_string();

                    if !is_evm_chain {
                        tracing::info!(trade_no=?tx_hash, "非EVM链，链上未找到该交易，等待下一轮Scanner重试");
                        return Ok(None);
                    }

                    let chain_nonce = match Self::nonce(from_addr, chain_code).await {
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

                    match Self::classify_evm_missing_tx_hash_state(chain_nonce, local_nonce) {
                        EvmRecoverMissingTxHashState::ChainNonceConsumedLocalHashMissing => {
                            tracing::warn!(
                                trade_no=?tx_hash,
                                chain_nonce = chain_nonce,
                                local_nonce = local_nonce,
                                "EVM recover: local tx hash missing but nonce already consumed on-chain; keep uncertain"
                            );
                            return Ok(None);
                        }
                        EvmRecoverMissingTxHashState::ChainNonceMatchesLocalNonce => {
                            tracing::warn!(
                                trade_no=?tx_hash,
                                chain_nonce = chain_nonce,
                                local_nonce = local_nonce,
                                "EVM recover: nonce matches next expected nonce but tx hash missing (rpc accepted only / not propagated / dropped)"
                            );
                            return Ok(None);
                        }
                        EvmRecoverMissingTxHashState::LocalNonceAheadOfChainNonce => {
                            tracing::warn!(
                                trade_no=?tx_hash,
                                chain_nonce = chain_nonce,
                                local_nonce = local_nonce,
                                "EVM recover: local nonce ahead of chain nonce (future nonce / nonce gap suspected)"
                            );
                            return Ok(None);
                        }
                    }
                }

                // === C. RPC 异常 ===
                Err(err) => {
                    let service_err: ServiceError = err.into();
                    if !auth_retry_attempted && service_err.is_rpc_auth_unauthorized() {
                        auth_retry_attempted = true;
                        Self::refresh_rpc_auth_and_prepare_retry(
                            chain_code,
                            "process_recovered_tx:query_tx_res",
                            Some(&rpc),
                            &service_err,
                        )
                        .await?;
                        continue 'recover_auth_retry;
                    }
                    if chain_rpc_guard::is_transient_chain_rpc_error_message(
                        &service_err.to_string(),
                    ) {
                        tracing::warn!(trade_no=?tx_hash, "查询链上状态失败(瞬时): {}", service_err);
                    } else {
                        tracing::error!(trade_no=?tx_hash, "查询链上状态失败: {}", service_err);
                    }
                    chain_rpc_guard::record_transient_failure_from_error(&service_err);
                    if auth_retry_attempted {
                        tracing::warn!(chain_code=%chain_code, rpc=%rpc, op="process_recovered_tx:query_tx_res", error=%service_err, "auth retry failed");
                    }
                    return Ok(None); // 容错，下轮再查
                }
            }
        }
    }
}
