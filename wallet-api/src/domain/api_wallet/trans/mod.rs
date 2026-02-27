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

    fn evm_raw_hash_hint(raw: &[u8]) -> String {
        let digest = Keccak256::digest(raw);
        format!("0x{}", hex::encode(digest))
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

    pub(crate) fn is_blockhash_not_found_error(err: &ServiceError) -> bool {
        let s = err.to_string().to_ascii_lowercase();
        s.contains("blockhash not found") || s.contains("block hash not found")
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
        tracing::info!(?params, "transfer: 构建原始交易");
        let resp = adapter.build_transfer_raw(&params, private_key).await?;
        tracing::info!("transfer: 转账操作完成, 耗时: {:?}", transfer_time.elapsed());
        tracing::info!(?resp, "transfer: 构建原始交易完成");

        Ok(resp)
    }

    /// ⚠️ 注意：
    /// 网络错误不等于广播失败。
    /// broadcast_transfer 在网络异常时返回 Ok(None)，
    /// 由 scanner / recover 决定最终状态。
    pub async fn broadcast_transfer(
        chain_code: &str,
        raw: RawTx,
    ) -> Result<Option<TransferResp>, ServiceError> {
        let start_time = Instant::now();
        tracing::info!("broadcast_transfer (开始): 链: {}, 时间: {:?}", chain_code, start_time);

        let tx_hash_hint = match &raw {
            RawTx::Tron(raw, ..) => Some(raw.tx_id.clone()),
            // For SOL we may already have signature/hash-like value in raw.
            RawTx::Sol(sig, ..) => Some(sig.clone()),
            RawTx::Evm(raw, ..) => Some(Self::evm_raw_hash_hint(raw)),
        };
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
                        let tx_hash = if let Some(tx_hash) = tx_hash_hint.as_ref().cloned() {
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

                    let time = tx_result.transaction_time;

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
                        mock_resp.with_transaction_time(time);
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

                    if chain_nonce > local_nonce {
                        tracing::warn!(
                            trade_no=?tx_hash,
                            chain_nonce = chain_nonce,
                            local_nonce = local_nonce,
                            "EVM recover: local tx hash missing but nonce already consumed on-chain (likely replaced/expired)"
                        );
                        return Err(ServiceError::System(
                            crate::error::system::SystemError::Internal(
                                "lost pending tx (nonce already consumed on-chain)".into(),
                            ),
                        ));
                    }

                    if chain_nonce == local_nonce {
                        tracing::warn!(
                            trade_no=?tx_hash,
                            chain_nonce = chain_nonce,
                            local_nonce = local_nonce,
                            "EVM recover: nonce matches next expected nonce but tx hash missing (rpc accepted only / not propagated / dropped)"
                        );
                        return Ok(None);
                    }

                    tracing::warn!(
                        trade_no=?tx_hash,
                        chain_nonce = chain_nonce,
                        local_nonce = local_nonce,
                        "EVM recover: local nonce ahead of chain nonce (future nonce / nonce gap suspected)"
                    );
                    return Ok(None);
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
