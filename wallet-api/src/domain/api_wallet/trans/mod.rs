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
use std::{fmt::Display, time::Instant};
use tokio::time::sleep;
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_types::chain::chain::ChainCode;
use wallet_utils::RetryableError as _;

pub(crate) mod broadcast_mechanism;
pub(crate) mod collect;
pub(crate) mod fee;
mod recover_flow;
mod recover_mechanism;
pub(crate) mod withdraw;

#[cfg(test)]
mod confirm_tx_tests;

#[cfg(test)]
mod chain_time_tests {
    use super::ApiTransDomain;

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

        let tx_hash_hint =
            broadcast_mechanism::select_broadcast_tx_hash_hint(&raw, expected_tx_hash);
        if let RawTx::Evm(raw, ..) = &raw {
            tracing::info!(
                chain_code = %chain_code,
                local_raw_hash = %broadcast_mechanism::evm_raw_hash_hint(raw),
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
            first_raw_attempt.as_ref().and_then(broadcast_mechanism::clone_raw_for_single_retry);
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
                    if broadcast_mechanism::is_duplicate_broadcast_error(&e) {
                        let tx_hash = if let Some(tx_hash) =
                            broadcast_mechanism::resolve_duplicate_tx_hash(
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
                        && broadcast_mechanism::is_transaction_expired_error(&e)
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
}
