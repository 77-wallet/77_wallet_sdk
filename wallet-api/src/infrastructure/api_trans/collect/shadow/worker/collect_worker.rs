#![allow(deprecated)]
// collect/shadow/worker/collect_worker.rs

// Architecture Rule:
// - Broadcast success MUST only update last_broadcast_at
// - transaction_time is an irreversible on-chain confirmation fact
// - Only Scanner / Shadow Recovery may write transaction_time
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use tracing::{error, info, warn};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool,
    entities::{
        api_collect::{ApiCollectEntity, ApiCollectStatus, ErrCode},
        asset_token_key::AssetTokenKey,
    },
    repositories::api_wallet::{
        account::ApiAccountRepo, collect::ApiCollectRepo, wallet::ApiWalletRepo,
    },
};
use wallet_transport_backend::request::api_wallet::strategy::ChainConfig;
use wallet_types::chain::chain::ChainCode;
use wallet_utils::{RetryableError as _, conversion, unit};

// 从crate::response_vo导入必要的Fee类型
use crate::{
    domain::api_wallet::wallet::ApiWalletDomain,
    error::{business::api_wallet::trans::TransError, system::SystemError},
    infrastructure::nonce::nonce_engine::{ReconcileReason, get_nonce_engine},
    request::api_wallet::trans::ApiTransferReq,
    response_vo::{CommonFeeDetails, EthereumFeeDetails, FeeDetailsVo, TronFeeDetails},
};

use crate::{
    domain::api_wallet::{
        adapter::tx::RawTx, adapter_factory::ApiChainAdapterFactory, chain::ApiChainTransDomain,
        coin::ApiCoinDomain, strategy::StrategyDomain, trans::ApiTransDomain,
    },
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    infrastructure::api_trans::collect::legacy::AddressLockManager,
    request::api_wallet::trans::ApiBaseTransferReq,
};

/// Shadow Worker Command 结构
/// 只表达："对某个 trade_no 执行某个确定动作"
#[derive(Debug)]
pub enum ShadowCollectCommand {
    /// 构建交易
    BuildTx(String),
    /// 广播交易
    Broadcast(String),
    /// 恢复交易
    Recover(String),
}

/// Shadow Worker
/// 纯执行型、无状态假设、可随时 kill -9 的 Worker
///
/// Shadow Worker 约束：
/// - 永远不发送 ACK
/// - 永远不做业务决策
/// - 永远只执行 DB 已确认允许的动作
/// - DB 是唯一真理源
/// - 可随时被 kill-9，不影响系统一致性
/// - 只负责执行链动作：build / broadcast / confirm
/// - 不依赖任何外部业务系统
/// - 不产生任何业务承诺
/// - 只执行链相关操作，不涉及业务逻辑
/// - 不做任何 in-flight 管理，并发与去重完全由 DB 状态机保证
///
/// Shadow Worker design invariant:
///
/// Phase 1: Address lock + fact arbitration (no network)
/// - 地址锁内进行并发裁决
/// - 分配 nonce（确保同一地址串行）
/// - 锁内禁止任何网络调用、sleep、await RPC
/// - 裁决依据必须基于锁内 fresh read
///
/// Phase 2: Network execution (no shared state)
/// - 锁外执行网络/RPC/构建/广播
/// - chain_rpc_guard 只限制外部世界并发
/// - 允许失败和重试
///
/// Phase 3: DB commit (with address lock)
/// - 持锁写事实，保证原子性
/// - 只写事实，不做决策
/// - 写事实后必须调用 try_advance 唤醒 Scanner
use crate::infrastructure::api_trans::collect::shadow::ShadowAdvancer;

pub struct ShadowCollectWorker {
    /// 数据库连接池
    collect_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
    /// 地址锁管理器，保护地址级并发
    address_locks: Arc<AddressLockManager>,
    /// ShadowAdvancer 引用，用于统一推进执行
    advancer: Arc<ShadowAdvancer>,
}

impl ShadowCollectWorker {
    const TRON_RAW_EXPIRY_GUARD_MS: i64 = 3_000;
    const TRON_MISSING_CONFIRMED_AND_PENDING_REBROADCAST_TIMEOUT_SECS: i64 = 5 * 60;
    const EVM_UNCERTAIN_TIMEOUT_SECS: i64 = 5 * 60;
    const EVM_UNCERTAIN_BACKOFF_MID_SECS: i64 = 15;
    const EVM_UNCERTAIN_BACKOFF_MAX_SECS: i64 = 30;
    const EVM_UNCERTAIN_AUTO_REBROADCAST_LIMIT: u32 = 1;
    const EVM_UNCERTAIN_AUTO_FAIL_ERR_CODE: ErrCode = ErrCode::TransactionOnChainException;
    const BUILD_SLOT_STALE_SECS: i64 = 30;

    fn is_evm_chain_code(chain_code: &str) -> bool {
        chain_code.eq_ignore_ascii_case("eth") || chain_code.eq_ignore_ascii_case("bnb")
    }

    fn should_spend_all_native_collect(chain_code: &str, token_key: &AssetTokenKey) -> bool {
        chain_code.eq_ignore_ascii_case("sol") && token_key.is_native()
    }

    fn collect_balance_need(
        fee: Decimal,
        value: Decimal,
        spend_all_native: bool,
    ) -> Result<Decimal, ServiceError> {
        if spend_all_native { Ok(fee) } else { Ok(fee + value) }
    }

    fn evm_uncertain_backoff_secs(retry_count: u32) -> i64 {
        match retry_count {
            0..=3 => 0,
            4..=6 => Self::EVM_UNCERTAIN_BACKOFF_MID_SECS,
            _ => Self::EVM_UNCERTAIN_BACKOFF_MAX_SECS,
        }
    }

    fn should_force_align_prebuild_nonce_gap(chain_nonce: u64, local_nonce: u64) -> bool {
        local_nonce > chain_nonce && local_nonce.saturating_sub(chain_nonce) >= 2
    }

    fn evm_uncertain_elapsed_secs(req: &ApiCollectEntity, now: DateTime<Utc>) -> Option<i64> {
        req.broadcast_uncertain_since_at
            .map(|since| now.signed_duration_since(since).num_seconds().max(0))
    }

    fn should_throttle_evm_uncertain_recover(req: &ApiCollectEntity, now: DateTime<Utc>) -> bool {
        let Some(since) = req.broadcast_uncertain_since_at else {
            return false;
        };
        let elapsed = now.signed_duration_since(since).num_seconds();
        if elapsed >= Self::EVM_UNCERTAIN_TIMEOUT_SECS {
            return false;
        }
        let Some(last_checked) = req.broadcast_uncertain_last_checked_at else {
            return false;
        };
        let wait_secs = Self::evm_uncertain_backoff_secs(req.broadcast_uncertain_retry_count);
        if wait_secs <= 0 {
            return false;
        }
        now.signed_duration_since(last_checked).num_seconds() < wait_secs
    }

    fn is_stale_build_slot(building_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
        let Some(building_at) = building_at else {
            return false;
        };

        now.signed_duration_since(building_at).num_seconds() >= Self::BUILD_SLOT_STALE_SECS
    }

    fn tron_raw_expiration_ms(raw_tx: &RawTx) -> Option<i64> {
        let RawTx::Tron(raw, ..) = raw_tx else { return None };
        let v: serde_json::Value = serde_json::from_str(&raw.raw_data).ok()?;
        v.get("expiration").and_then(|x| x.as_i64().or_else(|| x.as_u64().map(|u| u as i64)))
    }

    fn should_invalidate_expired_tron_raw(chain_code: &str, raw_tx_json: &str) -> bool {
        if !chain_code.eq_ignore_ascii_case("tron") {
            return false;
        }
        let raw_tx: RawTx = match wallet_utils::serde_func::serde_from_str(raw_tx_json) {
            Ok(raw_tx) => raw_tx,
            Err(_) => return false,
        };
        let Some(exp_ms) = Self::tron_raw_expiration_ms(&raw_tx) else {
            return false;
        };
        let now_ms = Utc::now().timestamp_millis();
        exp_ms <= now_ms.saturating_add(Self::TRON_RAW_EXPIRY_GUARD_MS)
    }

    fn should_nudge_advance_after_recover_skip(req: &ApiCollectEntity) -> bool {
        req.transaction_time.is_some()
    }

    fn normalized_tx_hash<'a>(req: &'a ApiCollectEntity) -> Option<&'a str> {
        req.tx_hash.as_deref().map(str::trim).filter(|s| !s.is_empty())
    }

    fn validate_recovered_tx_hash(
        trade_no: &str,
        existing_tx_hash: Option<&str>,
        recovered_tx_hash: &str,
    ) -> Result<(), ServiceError> {
        if let Some(existing_tx_hash) = existing_tx_hash {
            if recovered_tx_hash != existing_tx_hash {
                error!(
                    trade_no = %trade_no,
                    existing_tx_hash = %existing_tx_hash,
                    recover_tx_hash = %recovered_tx_hash,
                    source = "shadow_worker_v2",
                    "tx_hash mismatch during recover - fact integrity violated"
                );
                return Err(ServiceError::System(SystemError::Internal(
                    "recover tx_hash mismatch".to_string(),
                )));
            }
        }

        Ok(())
    }

    fn is_insufficient_fee_balance_error(err: &ServiceError) -> bool {
        matches!(
            err,
            ServiceError::Business(crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::InsufficientFeeBalance
            ))
        )
    }

    fn is_solana_rent_exempt_reserve_balance_error(
        req: &ApiCollectEntity,
        err: &ServiceError,
    ) -> bool {
        if !req.chain_code.eq_ignore_ascii_case("sol") {
            return false;
        }

        match err {
            ServiceError::Business(crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::InsufficientBalance(detail),
            )) => detail
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("rent-exempt reserve")),
            ServiceError::Business(crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::InsufficientFundsRent,
            )) => true,
            _ => false,
        }
    }

    fn is_tron_missing_confirmed_and_pending_error(err: &ServiceError) -> bool {
        err.to_string().contains("tron tx missing from confirmed and pending pools")
    }

    fn should_rebroadcast_tron_missing_confirmed_and_pending(req: &ApiCollectEntity) -> bool {
        if !req.chain_code.eq_ignore_ascii_case("tron") {
            return false;
        }
        let Some(last_broadcast_at) = req.last_broadcast_at else {
            return false;
        };
        Utc::now().signed_duration_since(last_broadcast_at).num_seconds()
            >= Self::TRON_MISSING_CONFIRMED_AND_PENDING_REBROADCAST_TIMEOUT_SECS
    }

    fn apply_exec_to_addr(req: &mut ApiCollectEntity, exec_to_addr: &str) -> bool {
        if req.to_addr == exec_to_addr {
            return false;
        }
        req.to_addr = exec_to_addr.to_string();
        true
    }

    /// 创建新的 Shadow Collect Worker
    pub fn new(
        pool: ApiTransactionDbPool,
        core_pool: ApiWalletDbPool,
        address_locks: Arc<AddressLockManager>,
        advancer: Arc<ShadowAdvancer>,
    ) -> Self {
        Self { collect_pool: pool, core_pool, address_locks, advancer }
    }

    /// 处理单个 Command
    pub async fn handle(&self, cmd: ShadowCollectCommand) -> Result<(), ServiceError> {
        // 提取 trade_no 用于日志
        let trade_no = match &cmd {
            ShadowCollectCommand::BuildTx(trade_no) => trade_no,
            ShadowCollectCommand::Broadcast(trade_no) => trade_no,
            ShadowCollectCommand::Recover(trade_no) => trade_no,
        };

        info!(trade_no = %trade_no, command = ?cmd, source = "shadow_worker_v2", "Received shadow collect command");

        match cmd {
            ShadowCollectCommand::BuildTx(trade_no) => self.process_build_tx(trade_no).await,
            ShadowCollectCommand::Broadcast(trade_no) => self.process_broadcast(trade_no).await,
            ShadowCollectCommand::Recover(trade_no) => self.process_recover(trade_no).await,
        }
    }

    /// 执行 Recover Command - 外层wrapper，确保所有错误都被捕获
    async fn process_recover(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "shadow_worker_v2", "Processing Recover command");

        // 使用内层函数来捕获所有错误
        if let Err(err) = self.process_recover_inner(&trade_no).await {
            error!(trade_no = %trade_no, error = %err, source = "shadow_worker_v2", "Recover inner failed, handling error");
            // 注意：recover 失败不写失败事实
            // recover 失败 = 不知道链上发生了什么，而不是"交易失败"
            // 因此不调用 handle_collect_tx_failed，让 Scanner 在下一次扫描时重新尝试
        }

        Ok(())
    }

    /// Recover 内部实现，可能返回错误
    async fn process_recover_inner(&self, trade_no: &str) -> Result<(), ServiceError> {
        // ====== phase 1: 锁内 · 并发裁决 ======
        // ⚠️ 锁内禁止任何网络调用、sleep、await RPC
        let req = {
            // 获取地址锁，保护地址级并发
            // 注意：initial_req 仅用于定位地址锁，不参与裁决
            // 所有裁决必须基于锁内的 fresh_req
            let initial_req = self.get_collect_entity(trade_no).await?;
            let _addr_guard = self.address_locks.acquire(&initial_req.from_addr).await?;
            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Acquired address lock");

            // 🔒 必须锁内重新读取，确保基于最新状态做决策
            // ⚠️ 只读"裁决字段"，不做任何业务推断
            let fresh_req = self.get_collect_entity(trade_no).await?;

            // 事实校验：Recover 只能处理 tx_hash 存在且 transaction_time 为空的交易
            // ⚠️ 这里是并发裁决的关键，确保只有一个task能通过
            if fresh_req.tx_hash.is_none() || fresh_req.transaction_time.is_some() {
                info!(trade_no = %trade_no, source = "shadow_worker_v2", "tx_hash empty or transaction_time exists, skipping Recover");
                return Ok(());
            }

            if Self::is_evm_chain_code(&fresh_req.chain_code)
                && fresh_req.raw_tx.is_some()
                && fresh_req.last_broadcast_at.is_none()
                && fresh_req.broadcast_uncertain_since_at.is_none()
                && fresh_req.transaction_time.is_none()
                && fresh_req.tx_exec_receipt_uploaded_at.is_none()
            {
                info!(
                    trade_no = %trade_no,
                    tx_hash = %fresh_req.tx_hash.as_deref().unwrap_or_default(),
                    nonce = fresh_req.nonce,
                    source = "shadow_worker_v2",
                    "Skip Recover: EVM raw_tx exists but not in uncertain state; broadcast should proceed"
                );
                return Ok(());
            }

            fresh_req
        };
        // 🔓 锁在这里已经释放

        if Self::is_evm_chain_code(&req.chain_code) {
            let now = Utc::now();
            if Self::should_throttle_evm_uncertain_recover(&req, now) {
                let elapsed = Self::evm_uncertain_elapsed_secs(&req, now).unwrap_or_default();
                let wait_secs =
                    Self::evm_uncertain_backoff_secs(req.broadcast_uncertain_retry_count);
                let since_last = req
                    .broadcast_uncertain_last_checked_at
                    .map(|ts| now.signed_duration_since(ts).num_seconds().max(0))
                    .unwrap_or_default();
                info!(
                    trade_no = %req.trade_no,
                    tx_hash = %req.tx_hash.as_deref().unwrap_or_default(),
                    nonce = req.nonce,
                    retry_count = req.broadcast_uncertain_retry_count,
                    elapsed_secs = elapsed,
                    since_last_check_secs = since_last,
                    backoff_wait_secs = wait_secs,
                    source = "shadow_worker_v2",
                    "Skip recover query due to EVM uncertain backoff"
                );
                return Ok(());
            }
        }

        // ====== phase 2: 锁外 · 网络执行 ======
        // 获取链交互全局许可（按 guarded endpoint 控制并发）
        let _chain_rpc_guard =
            crate::infrastructure::chain_rpc_guard::acquire_if_guarded(&req.chain_code).await;
        if _chain_rpc_guard.is_some() {
            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Acquired chain rpc guard permit");
        }

        // 执行恢复交易
        let recover_result = self.recover_tx(&req).await;
        match recover_result {
            Ok(Some(tx_resp)) => {
                info!(trade_no = %trade_no, tx_hash = %tx_resp.tx_hash, source = "shadow_worker_v2", "Transaction recover successful");

                // ====== phase 3: 锁内 · 提交不可逆事实 ======
                {
                    // 重新获取地址锁，保护地址级并发
                    let _addr_guard = self.address_locks.acquire(&req.from_addr).await?;
                    info!(trade_no = %trade_no, source = "shadow_worker_v2", "Reacquired address lock for fact commit");

                    // 🔒 必须锁内重新读取，确保基于最新状态做决策
                    let fresh_req = self.get_collect_entity(trade_no).await?;

                    let existing_tx_hash = Self::normalized_tx_hash(&fresh_req);

                    // 事实校验：如果链上确认已经落库，补齐缺失的 tx_hash 后直接推进后续阶段
                    if fresh_req.transaction_time.is_some() {
                        let mut should_nudge =
                            Self::should_nudge_advance_after_recover_skip(&fresh_req);

                        if existing_tx_hash.is_none() {
                            let rows_affected = ApiCollectRepo::backfill_tx_hash_if_missing(
                                &self.collect_pool,
                                &fresh_req.trade_no,
                                &tx_resp.tx_hash,
                                "shadow_worker_v2",
                            )
                            .await
                            .map_err(|e| ServiceError::Database(e.into()))?;

                            info!(
                                trade_no = %fresh_req.trade_no,
                                tx_hash = %tx_resp.tx_hash,
                                rows_affected = %rows_affected,
                                source = "shadow_worker_v2",
                                "Recovered tx hash backfilled after concurrent missing-hash read"
                            );

                            if rows_affected > 0 {
                                self.advancer.try_advance(&fresh_req.trade_no).await;
                                should_nudge = false;
                            }
                        }

                        info!(trade_no = %trade_no, source = "shadow_worker_v2", "tx_hash empty or transaction_time exists, skipping Recover fact commit");
                        if should_nudge {
                            info!(
                                trade_no = %trade_no,
                                source = "shadow_worker_v2",
                                "Recover facts already present; nudging advancer for downstream stages"
                            );
                            self.advancer.try_advance(&fresh_req.trade_no).await;
                        }
                        return Ok(());
                    }

                    // 🔒 事实保护：检查 tx_hash 一致性，防止事实被覆盖
                    Self::validate_recovered_tx_hash(
                        &fresh_req.trade_no,
                        existing_tx_hash,
                        &tx_resp.tx_hash,
                    )?;

                    // 使用链上时间设置 transaction_time
                    // 必须使用链返回的时间，禁止使用本地时间作为后备
                    let transaction_time_ms = tx_resp.transaction_time_ms.ok_or_else(|| {
                        ServiceError::System(SystemError::Internal(
                            "recover_tx returned final result but missing transaction_time_ms"
                                .to_string(),
                        ))
                    })?;

                    // 将毫秒转换为ISO 8601格式
                    let transaction_time =
                        chrono::DateTime::<Utc>::from_timestamp_millis(transaction_time_ms as i64)
                            .ok_or_else(|| {
                                ServiceError::System(SystemError::Internal(
                                    "invalid transaction_time_ms from chain".to_string(),
                                ))
                            })?
                            .to_rfc3339();
                    let resource_consume = if let Some(consumer) = tx_resp.consumer {
                        consumer.energy_used.to_string()
                    } else {
                        "0".to_string()
                    };

                    let rows_affected =
                        ApiCollectRepo::confirm_onchain_transaction_fact_with_recover(
                            &self.collect_pool,
                            &fresh_req.trade_no,
                            &tx_resp.tx_hash,
                            &transaction_time,
                            &transaction_time,
                            &tx_resp.fee,
                            &resource_consume,
                        )
                        .await
                        .map_err(|e| ServiceError::Database(e.into()))?;

                    // 显式处理幂等情况：恢复已被其他并发执行
                    if rows_affected == 0 {
                        info!(
                            trade_no = %fresh_req.trade_no,
                            tx_hash = %tx_resp.tx_hash,
                            source = "shadow_worker_v2",
                            "confirm_onchain_transaction_fact_with_recover skipped: recover already executed (idempotent hit)"
                        );
                    } else {
                        // 直接调用 try_advance 进行点对点唤醒
                        self.advancer.try_advance(&fresh_req.trade_no).await;
                    }
                }
            }
            Ok(None) => {
                if let Some(raw_tx_json) = req.raw_tx.as_deref() {
                    if req.last_broadcast_at.is_none()
                        && Self::should_invalidate_expired_tron_raw(&req.chain_code, raw_tx_json)
                    {
                        warn!(
                            trade_no = %req.trade_no,
                            tx_hash = %req.tx_hash.as_deref().unwrap_or_default(),
                            reason_code = "recover_expired_raw",
                            source = "shadow_worker_v2",
                            "Detected expired tron raw_tx during recover; invalidating stale tx facts"
                        );
                        info!(
                            trade_no = %req.trade_no,
                            source = "shadow_worker_v2",
                            "Using rebuild-only invalidation path for expired raw_tx (will NOT set need_service_fee)"
                        );
                        let rows = ApiCollectRepo::invalidate_raw_tx_for_rebuild(
                            &self.collect_pool,
                            &req.trade_no,
                            None,
                        )
                        .await
                        .map_err(|e| ServiceError::Database(e.into()))?;
                        if rows > 0 {
                            self.advancer.try_advance(&req.trade_no).await;
                        }
                        return Ok(());
                    }
                }

                info!(trade_no = %trade_no, source = "shadow_worker_v2", "Transaction recover result is uncertain");
                if !Self::is_evm_chain_code(&req.chain_code) {
                    // 查链不确定（含链上查不到 hash）后，立即尝试推进一次；
                    // 若满足广播条件会直接进入 Broadcast 重试，避免纯等待下一轮定时扫描。
                    self.advancer.try_advance(trade_no).await;
                    return Ok(());
                }

                let now = Utc::now();
                let rows_affected =
                    ApiCollectRepo::mark_broadcast_uncertain_attempt(&self.collect_pool, trade_no)
                        .await
                        .map_err(|e| ServiceError::Database(e.into()))?;
                let refreshed = self.get_collect_entity(trade_no).await?;
                info!(
                    trade_no = %refreshed.trade_no,
                    tx_hash = %refreshed.tx_hash.as_deref().unwrap_or_default(),
                    nonce = refreshed.nonce,
                    rows_affected = %rows_affected,
                    retry_count = refreshed.broadcast_uncertain_retry_count,
                    uncertain_since_at = ?refreshed.broadcast_uncertain_since_at,
                    reconciled_at = ?refreshed.broadcast_uncertain_reconciled_at,
                    rebroadcast_count = refreshed.broadcast_uncertain_rebroadcast_count,
                    source = "shadow_worker_v2",
                    "EVM recover uncertain state recorded"
                );

                let elapsed_secs =
                    Self::evm_uncertain_elapsed_secs(&refreshed, now).unwrap_or_default();
                let timed_out = elapsed_secs >= Self::EVM_UNCERTAIN_TIMEOUT_SECS;
                if !timed_out {
                    return Ok(());
                }

                let local_nonce_for_log = refreshed.nonce as u64;
                let mut chain_nonce_for_log: Option<u64> = None;
                let mut reconcile_reason_label = "already_reconciled";

                if refreshed.broadcast_uncertain_reconciled_at.is_none() {
                    warn!(
                        trade_no = %refreshed.trade_no,
                        tx_hash = %refreshed.tx_hash.as_deref().unwrap_or_default(),
                        nonce = refreshed.nonce,
                        uncertain_duration_sec = elapsed_secs,
                        source = "shadow_worker_v2",
                        "EVM uncertain timeout reached; running nonce reconcile"
                    );

                    let nonce_engine = get_nonce_engine();
                    let chain_nonce =
                        ApiTransDomain::nonce(&refreshed.from_addr, &refreshed.chain_code).await?;
                    chain_nonce_for_log = Some(chain_nonce);

                    if chain_nonce < local_nonce_for_log {
                        let gap = local_nonce_for_log.saturating_sub(chain_nonce);
                        warn!(
                            trade_no = %refreshed.trade_no,
                            from_addr = %refreshed.from_addr,
                            tx_hash = %refreshed.tx_hash.as_deref().unwrap_or_default(),
                            chain_nonce = %chain_nonce,
                            local_nonce = %local_nonce_for_log,
                            gap = %gap,
                            source = "shadow_worker_v2",
                            "EVM uncertain nonce-gap detected; forcing local nonce to chain nonce"
                        );
                        let _aligned_chain_next = nonce_engine
                            .force_align_to_chain_next_nonce(
                                &refreshed.from_addr,
                                &refreshed.chain_code,
                                ReconcileReason::Other("evm_uncertain_nonce_gap".to_string()),
                            )
                            .await?;
                        reconcile_reason_label = "nonce_gap";
                    } else {
                        nonce_engine.trigger_reconcile_with_reason(
                            &refreshed.from_addr,
                            &refreshed.chain_code,
                            ReconcileReason::Other("evm_uncertain_timeout".to_string()),
                            true,
                        );
                        reconcile_reason_label = "generic_uncertain";
                    }
                    let _ = ApiCollectRepo::mark_broadcast_uncertain_reconciled(
                        &self.collect_pool,
                        &refreshed.trade_no,
                    )
                    .await
                    .map_err(|e| ServiceError::Database(e.into()))?;
                }

                if refreshed.broadcast_uncertain_rebroadcast_count
                    < Self::EVM_UNCERTAIN_AUTO_REBROADCAST_LIMIT
                {
                    warn!(
                        trade_no = %refreshed.trade_no,
                        tx_hash = %refreshed.tx_hash.as_deref().unwrap_or_default(),
                        nonce = refreshed.nonce,
                        reason = reconcile_reason_label,
                        chain_nonce = ?chain_nonce_for_log,
                        local_nonce = %local_nonce_for_log,
                        decision = "rebuild_retry_once",
                        source = "shadow_worker_v2",
                        "EVM uncertain reconcile decision"
                    );
                    let rows = ApiCollectRepo::invalidate_raw_tx_for_rebuild(
                        &self.collect_pool,
                        &refreshed.trade_no,
                        None,
                    )
                    .await
                    .map_err(|e| ServiceError::Database(e.into()))?;
                    if rows > 0 {
                        let _ = ApiCollectRepo::mark_broadcast_uncertain_rebroadcast_attempted(
                            &self.collect_pool,
                            &refreshed.trade_no,
                        )
                        .await
                        .map_err(|e| ServiceError::Database(e.into()))?;
                        self.advancer.try_advance(&refreshed.trade_no).await;
                    }
                    return Ok(());
                }

                warn!(
                    trade_no = %refreshed.trade_no,
                    tx_hash = %refreshed.tx_hash.as_deref().unwrap_or_default(),
                    nonce = refreshed.nonce,
                    uncertain_duration_sec = elapsed_secs,
                    reconcile_done = %refreshed.broadcast_uncertain_reconciled_at.is_some(),
                    rebroadcast_count = refreshed.broadcast_uncertain_rebroadcast_count,
                    source = "shadow_worker_v2",
                    "EVM uncertain exhausted; auto fail order"
                );

                let rows_affected = ApiCollectRepo::update_api_collect_status_and_err(
                    &self.collect_pool,
                    &refreshed.trade_no,
                    ApiCollectStatus::SendingTxFailed,
                    Self::EVM_UNCERTAIN_AUTO_FAIL_ERR_CODE,
                    "EVM broadcast uncertain timeout after 5m; same-rpc tx not visible; reconcile+1 retry exhausted",
                )
                .await
                .map_err(|db_err: wallet_database::Error| {
                    error!(trade_no = %refreshed.trade_no, error = %db_err, source = "shadow_worker_v2", "Failed to mark EVM uncertain timeout auto-fail");
                    ServiceError::Database(db_err.into())
                })?;
                if rows_affected > 0 {
                    self.advancer.try_advance(&refreshed.trade_no).await;
                }
            }
            Err(err) if Self::is_tron_missing_confirmed_and_pending_error(&err) => {
                if !Self::should_rebroadcast_tron_missing_confirmed_and_pending(&req) {
                    info!(
                        trade_no = %trade_no,
                        tx_hash = %req.tx_hash.as_deref().unwrap_or_default(),
                        source = "shadow_worker_v2",
                        "Tron tx missing from confirmed and pending pools; keep observing before rebroadcast"
                    );
                    return Ok(());
                }

                warn!(
                    trade_no = %trade_no,
                    tx_hash = %req.tx_hash.as_deref().unwrap_or_default(),
                    source = "shadow_worker_v2",
                    "Tron tx missing from confirmed and pending pools beyond timeout; rebroadcasting"
                );
                let rows = ApiCollectRepo::invalidate_raw_tx_for_rebroadcast(
                    &self.collect_pool,
                    &req.trade_no,
                    None,
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
                if rows > 0 {
                    self.advancer.try_advance(&req.trade_no).await;
                }
            }
            Err(err) => return Err(err),
        }

        Ok(())
    }

    /// 执行 BuildTx Command - 外层wrapper，确保所有错误都被捕获
    async fn process_build_tx(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "shadow_worker_v2", "Processing BuildTx command");

        // 使用内层函数来捕获所有错误
        if let Err(err) = self.process_build_tx_inner(&trade_no).await {
            error!(trade_no = %trade_no, error = %err, source = "shadow_worker_v2", "BuildTx inner failed, handling error");
            self.handle_collect_tx_failed(&trade_no, err).await?;
        }

        Ok(())
    }

    /// BuildTx 内部实现，可能返回错误
    async fn process_build_tx_inner(&self, trade_no: &str) -> Result<(), ServiceError> {
        // 1. 从数据库中获取归集交易信息
        let mut req = self.get_collect_entity(trade_no).await?;

        // 2. 事实校验：BuildTx 只能处理 raw_tx 为空的交易
        if req.raw_tx.is_some() {
            if req.need_service_fee == Some(true) {
                error!(
                    trade_no = %trade_no,
                    source = "shadow_worker_v2",
                    "Invariant violated: raw_tx exists while need_service_fee is true"
                );
                return Err(ServiceError::System(SystemError::Internal(
                    "Invariant violated: raw_tx exists while need_service_fee is true".to_string(),
                )));
            }
            info!(trade_no = %trade_no, source = "shadow_worker_v2", "raw_tx already exists, skipping BuildTx");
            return Ok(());
        }

        // 3. 事实校验：如果已有 tx_hash 且 transaction_time 为空，跳过 BuildTx
        // Recover 逻辑已移至独立的 Recover Command，由 Scanner 触发
        if req.tx_hash.is_some() && req.transaction_time.is_none() {
            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Found existing tx_hash without transaction_time, skipping BuildTx (recover will be handled by Scanner)");
            return Ok(());
        }

        // 4. 先占位建单槽位，防止同一 trade_no 在构建期间被重复推进
        // building_at 只作为短期 in-flight 保护，不参与最终事实判断。
        let build_slot_rows =
            ApiCollectRepo::update_building_at(&self.collect_pool, trade_no).await?;
        if build_slot_rows == 0 {
            if self.reclaim_stale_build_slot(trade_no, req.building_at).await? {
                let reclaimed_build_slot_rows =
                    ApiCollectRepo::update_building_at(&self.collect_pool, trade_no).await?;
                if reclaimed_build_slot_rows > 0 {
                    info!(
                        trade_no = %trade_no,
                        source = "shadow_worker_v2",
                        "Reclaimed stale build slot, continuing BuildTx"
                    );
                } else {
                    info!(
                        trade_no = %trade_no,
                        source = "shadow_worker_v2",
                        "Build slot still unavailable after stale reclaim attempt, skipping BuildTx"
                    );
                    return Ok(());
                }
            } else {
                info!(
                    trade_no = %trade_no,
                    source = "shadow_worker_v2",
                    "Build slot already claimed or recently updated, skipping BuildTx"
                );
                return Ok(());
            }
        }

        // 5. 解析执行地址 - 在执行期解析，支持重试
        let exec_to_addr = self.resolve_collect_to_addr(&req).await?;
        let latest_strategy_to = exec_to_addr.clone();
        let updated_to_addr = Self::apply_exec_to_addr(&mut req, &exec_to_addr);
        info!(
            trade_no = %trade_no,
            latest_strategy_to = %latest_strategy_to,
            persisted_exec_to_addr = %req.to_addr,
            updated_to_addr = %updated_to_addr,
            source = "shadow_worker_v2",
            "Resolved execution address for current build"
        );

        if updated_to_addr {
            ApiCollectRepo::update_api_collect_to_addr(
                &self.collect_pool,
                &req.trade_no,
                &exec_to_addr,
            )
            .await?;
            info!(
                trade_no = %trade_no,
                latest_strategy_to = %latest_strategy_to,
                persisted_exec_to_addr = %req.to_addr,
                source = "shadow_worker_v2",
                "Updated persisted execution address in database"
            );
        }

        // 6. 检查手续费
        //
        // ⚠️ IMPORTANT:
        // Fee insufficient is NOT a retryable failure.
        // It invalidates the current build facts and must go through invalidate_raw_tx.
        // Do NOT introduce any logic that only sets build_blocked_at.
        if !self.check_fee(&req).await? {
            info!(
                trade_no = %trade_no,
                reason_code = "fee_check_failed",
                source = "shadow_worker_v2",
                "Fee insufficient, invalidating current build attempt"
            );

            // 🔒 事实作废：如果 fee cycle 已经走完，只回滚可重建事实，不要把
            // need_service_fee 再次打回 true；否则会把已完成 fee cycle 的单子
            // 卡回“等待手续费结果”的死状态。
            let affected = self.invalidate_build_attempt_after_fee_check_failure(&req).await?;

            if affected == 0 {
                info!(
                    trade_no = %trade_no,
                    source = "shadow_worker_v2",
                    "Transaction already invalidated or no raw_tx to invalidate, skip"
                );
                self.clear_build_slot_after_claim(trade_no).await?;
            } else {
                // 直接调用 try_advance 进行点对点唤醒
                self.advancer.try_advance(&req.trade_no).await;
            }

            return Ok(());
        }
        info!(trade_no = %trade_no, source = "shadow_worker_v2", "Fee check passed");

        // 7. 检查交易摘要 - 仍然使用后端原始 digest 语义，不依赖当前执行地址
        if !self.check_digest(&req).await? {
            tracing::error!(trade_no=%trade_no, "collect_tx:send: 交易摘要验证失败");
            return Err(ServiceError::Business(
                ApiWalletError::Trans(TransError::TransactionDigestVerificationFailed).into(),
            ));
        }
        info!(trade_no = %trade_no, source = "shadow_worker_v2", "Transaction digest verification passed");

        // ====== phase 1: 锁内 · 快速检查 ======
        // ⚠️ 锁内禁止任何网络调用、sleep、await RPC
        let mut nonce = {
            // 获取地址锁，保护地址级并发
            let _addr_guard = self.address_locks.acquire(&req.from_addr).await?;
            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Acquired address lock");

            // 🔒 必须锁内重新读取，确保基于最新状态做决策
            // ⚠️ 只读"裁决字段"，不做任何业务推断
            let fresh_req = self.get_collect_entity(trade_no).await?;

            // 事实校验：BuildTx 只能处理 raw_tx 为空的交易
            // ⚠️ 这里是并发裁决的关键，确保只有一个task能通过
            if fresh_req.raw_tx.is_some() {
                info!(trade_no = %trade_no, source = "shadow_worker_v2", "raw_tx already exists, skipping BuildTx");
                self.clear_build_slot_after_claim(trade_no).await?;
                return Ok(());
            }

            // 获取并更新 nonce - 使用唯一入口 upsert_and_get_api_nonce
            // ⚠️ nonce 获取必须在锁内，确保同一地址的 nonce 串行化
            let nonce = self.get_nonce(&fresh_req.from_addr, &fresh_req.chain_code).await?;
            info!(trade_no = %trade_no, nonce = %nonce, source = "shadow_worker_v2", "Retrieved nonce");

            nonce
        };
        // 🔓 锁在这里已经释放

        // ====== phase 1.5: 锁外 · EVM nonce-gap 前置纠偏（单次最多一次） ======
        if Self::is_evm_chain_code(&req.chain_code) {
            let chain_nonce = ApiTransDomain::nonce(&req.from_addr, &req.chain_code).await?;
            if Self::should_force_align_prebuild_nonce_gap(chain_nonce, nonce) {
                let old_nonce = nonce;
                let gap = old_nonce.saturating_sub(chain_nonce);
                warn!(
                    trade_no = %trade_no,
                    from_addr = %req.from_addr,
                    chain_code = %req.chain_code,
                    chain_nonce = %chain_nonce,
                    local_nonce = %old_nonce,
                    gap = %gap,
                    source = "shadow_worker_v2",
                    "EVM pre-build nonce-gap detected; forcing local nonce to chain nonce"
                );

                let nonce_engine = get_nonce_engine();
                let _ = nonce_engine
                    .force_align_to_chain_next_nonce(
                        &req.from_addr,
                        &req.chain_code,
                        ReconcileReason::Other("evm_prebuild_nonce_gap".to_string()),
                    )
                    .await?;

                nonce = {
                    let _addr_guard = self.address_locks.acquire(&req.from_addr).await?;
                    let fresh_req = self.get_collect_entity(trade_no).await?;

                    if fresh_req.raw_tx.is_some() {
                        info!(
                            trade_no = %trade_no,
                            source = "shadow_worker_v2",
                            "raw_tx already exists, skipping BuildTx after pre-build nonce align"
                        );
                        self.clear_build_slot_after_claim(trade_no).await?;
                        return Ok(());
                    }

                    let new_nonce =
                        self.get_nonce(&fresh_req.from_addr, &fresh_req.chain_code).await?;
                    info!(
                        trade_no = %trade_no,
                        old_nonce = %old_nonce,
                        new_nonce = %new_nonce,
                        source = "shadow_worker_v2",
                        "Retrieved nonce after EVM pre-build force-align"
                    );
                    new_nonce
                };
            }
        }

        // ====== phase 2: 锁外 · 网络执行 ======
        // 获取链交互全局许可（按 guarded endpoint 控制并发）
        let _chain_rpc_guard =
            crate::infrastructure::chain_rpc_guard::acquire_if_guarded(&req.chain_code).await;
        if _chain_rpc_guard.is_some() {
            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Acquired chain rpc guard permit");
        }

        // 通过Context获取Handles实例，然后获取私钥管理器
        let handles = crate::context::get_context()?.get_handles_arc().await?;
        let private_key_manager = handles.get_global_private_key_manager();
        let private_key =
            private_key_manager.get_private_key(&req.from_addr, &req.chain_code).await?;
        info!(trade_no = %trade_no, source = "shadow_worker_v2", "Retrieved private key from manager");

        // 生成转账请求 - 使用解析后的执行地址和获取到的nonce
        // ⚠️ nonce 只在 phase 1 分配，这里直接传入
        let transfer_req = self.gen_transfer_req(&req, &exec_to_addr, nonce).await?;
        info!(trade_no = %trade_no, nonce = %nonce, source = "shadow_worker_v2", "Generated transfer request with nonce");

        // 构建交易
        let (tx_hash, raw_tx, fee) =
            crate::domain::api_wallet::trans::ApiTransDomain::build_transfer_raw(
                transfer_req,
                Some(private_key),
            )
            .await?;
        info!(trade_no = %trade_no, tx_hash = %tx_hash, fee = %fee, source = "shadow_worker_v2", "Built transfer raw transaction successfully");

        // ====== phase 3: 锁内 · 提交不可逆事实 ======
        {
            // 重新获取地址锁，保护地址级并发
            let _addr_guard = self.address_locks.acquire(&req.from_addr).await?;
            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Reacquired address lock for fact commit");

            // 🔒 必须锁内重新读取，确保基于最新状态做决策
            // ⚠️ 与 Broadcast / Recover 对齐，遵循"锁内 fresh read + 条件更新"铁律
            let fresh_req = self.get_collect_entity(trade_no).await?;

            // 事实校验：BuildTx 只能处理 raw_tx 为空的交易
            if fresh_req.raw_tx.is_some() {
                info!(trade_no = %trade_no, source = "shadow_worker_v2", "raw_tx already exists, skipping BuildTx fact commit");
                self.clear_build_slot_after_claim(trade_no).await?;
                return Ok(());
            }

            // 立即将tx_hash和raw_tx存储到数据库
            // 注意：使用序列化而非格式化，避免格式问题
            let raw_tx_str = wallet_utils::serde_func::serde_to_string(&raw_tx)?;
            let rows_affected = ApiCollectRepo::update_after_build(
                &self.collect_pool,
                &fresh_req.trade_no,
                &tx_hash,
                &raw_tx_str,
                &fee,
                nonce as i64,
            )
            .await?;

            // 显式处理幂等情况：如果影响行数为0，表示raw_tx已存在或被并发写入
            if rows_affected == 0 {
                info!(trade_no = %trade_no, source = "shadow_worker_v2", "update_after_build skipped: raw_tx already exists (idempotent hit)");
                self.clear_build_slot_after_claim(trade_no).await?;
                return Ok(());
            }

            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Updated tx_hash and raw_tx to database successfully");

            // 直接调用 try_advance 进行点对点唤醒
            self.advancer.try_advance(&fresh_req.trade_no).await;
        }

        // BuildTx命令完成，不负责广播，由Broadcast命令处理
        Ok(())
    }

    /// 执行 Broadcast Command - 外层wrapper，确保所有错误都被捕获
    async fn process_broadcast(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "shadow_worker_v2", "Processing Broadcast command");

        // 使用内层函数来捕获所有错误
        if let Err(err) = self.process_broadcast_inner(&trade_no).await {
            error!(trade_no = %trade_no, error = %err, source = "shadow_worker_v2", "Broadcast inner failed, handling error");
            self.handle_collect_tx_failed(&trade_no, err).await?;
        }

        Ok(())
    }

    /// Broadcast 内部实现，可能返回错误
    async fn process_broadcast_inner(&self, trade_no: &str) -> Result<(), ServiceError> {
        // ====== phase 1: 锁内 · 快速检查 ======
        // ⚠️ 锁内禁止任何网络调用、sleep、await RPC
        let req = {
            // 先获取初始的 collect 实体，用于获取 from_addr
            let initial_req = self.get_collect_entity(trade_no).await?;

            // 获取地址锁，保护地址级并发
            let _addr_guard = self.address_locks.acquire(&initial_req.from_addr).await?;
            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Acquired address lock for broadcast");

            // 🔒 必须锁内重新读取，确保基于最新状态做决策
            // ⚠️ 只读"裁决字段"，不做任何业务推断
            let fresh_req = self.get_collect_entity(trade_no).await?;

            // 事实校验：Broadcast 只能处理 raw_tx 存在且 transaction_time 为空的交易
            if fresh_req.raw_tx.is_none() || fresh_req.transaction_time.is_some() {
                info!(trade_no = %trade_no, source = "shadow_worker_v2", "raw_tx empty or transaction_time exists, skipping Broadcast");
                return Ok(());
            }

            // 事实校验：Broadcast 成功只应写入 last_broadcast_at，且必须是幂等的
            // ⚠️ IMPORTANT:
            // Broadcast success MUST only write last_broadcast_at
            // and MUST be idempotent (WHERE last_broadcast_at IS NULL)
            if fresh_req.last_broadcast_at.is_some() {
                info!(trade_no = %trade_no, source = "shadow_worker_v2", "last_broadcast_at already exists, skipping Broadcast");
                return Ok(());
            }

            if Self::is_evm_chain_code(&fresh_req.chain_code)
                && fresh_req.broadcast_uncertain_since_at.is_some()
                && fresh_req.transaction_time.is_none()
                && fresh_req.tx_exec_receipt_uploaded_at.is_none()
                && fresh_req.err_code.is_none()
            {
                info!(
                    trade_no = %trade_no,
                    source = "shadow_worker_v2",
                    "Skip Broadcast: EVM uncertain state in progress; recover should proceed"
                );
                return Ok(());
            }

            fresh_req
        };
        // 🔓 锁在这里已经释放

        // ====== phase 2: 锁外 · 网络执行 ======
        // 获取链交互全局许可（按 guarded endpoint 控制并发）
        let _chain_rpc_guard =
            crate::infrastructure::chain_rpc_guard::acquire_if_guarded(&req.chain_code).await;
        if _chain_rpc_guard.is_some() {
            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Acquired chain rpc guard permit for broadcast");
        }

        // 检查是否已有raw_tx和tx_hash
        if req.tx_hash.is_none() || req.raw_tx.is_none() || req.raw_tx.as_ref().unwrap().is_empty()
        {
            error!(trade_no = %trade_no, source = "shadow_worker_v2", "No raw_tx or tx_hash found");
            return Err(ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::ApiWalletError::Trans(
                        crate::error::business::api_wallet::trans::TransError::BuildWithdrawTransactionFailed("Missing transaction data".to_string()),
                    ),
                ),
            ));
        }

        // 反序列化raw_tx
        // 从数据库中获取的raw_tx是字符串格式，需要反序列化为RawTx类型
        let raw_tx = wallet_utils::serde_func::serde_from_str(req.raw_tx.as_deref().unwrap())?;
        info!(trade_no = %trade_no, tx_hash = %req.tx_hash.as_deref().unwrap(), source = "shadow_worker_v2", "Deserialized raw_tx successfully");

        // 广播交易
        info!(trade_no = %trade_no, tx_hash = %req.tx_hash.as_deref().unwrap(), source = "shadow_worker_v2", "Starting to broadcast transaction");
        let tx_resp = crate::domain::api_wallet::trans::ApiTransDomain::broadcast_transfer(
            &req.chain_code,
            raw_tx,
            req.tx_hash.as_deref(),
        )
        .await?;

        match tx_resp {
            Some(tx) => {
                info!(trade_no = %trade_no, tx_hash = %tx.tx_hash, source = "shadow_worker_v2", "Transaction broadcast successful");

                // ====== phase 3: 锁内 · 提交不可逆事实 ======
                {
                    // 重新获取地址锁，保护地址级并发
                    let _addr_guard = self.address_locks.acquire(&req.from_addr).await?;
                    info!(trade_no = %trade_no, source = "shadow_worker_v2", "Reacquired address lock for broadcast fact commit");

                    // 🔒 必须锁内重新读取，确保基于最新状态做决策
                    // ⚠️ Phase 3 永远只相信"锁内刚读出来的实体"
                    // Phase 1 / Phase 2 的 req 只能当上下文，不是事实来源
                    let fresh_req = self.get_collect_entity(trade_no).await?;

                    // 🔒 事实保护：检查 tx_hash 一致性，防止 build 阶段事实被覆盖
                    // 确保 build 阶段确立的 tx_hash 事实在 broadcast 阶段不被改写
                    if let Some(existing) = &fresh_req.tx_hash {
                        if existing != &tx.tx_hash {
                            error!(
                                trade_no = %fresh_req.trade_no,
                                existing_tx_hash = %existing,
                                broadcast_tx_hash = %tx.tx_hash,
                                source = "shadow_worker_v2",
                                "tx_hash mismatch between build and broadcast - fact integrity violated"
                            );
                            return Err(ServiceError::System(
                                crate::error::system::SystemError::Internal(
                                    "Invariant broken - tx_hash mismatch between build and broadcast"
                                        .to_string(),
                                ),
                            ));
                        }
                    }

                    // 广播成功 = 一次不可分割的事实提交
                    let resource_consume = if let Some(consumer) = tx.consumer {
                        consumer.energy_used.to_string()
                    } else {
                        "0".to_string()
                    };

                    let rows_affected = ApiCollectRepo::mark_broadcast_executed(
                        &self.collect_pool,
                        &fresh_req.trade_no,
                    )
                    .await
                    .map_err(|e| ServiceError::Database(e.into()))?;

                    // 显式处理幂等情况：广播已被其他并发/恢复执行
                    if rows_affected == 0 {
                        info!(
                            trade_no = %fresh_req.trade_no,
                            tx_hash = %tx.tx_hash,
                            source = "shadow_worker_v2",
                            "mark_broadcast_executed skipped: broadcast already executed (idempotent hit)"
                        );
                    } else {
                        // 直接调用 try_advance 进行点对点唤醒
                        self.advancer.try_advance(&fresh_req.trade_no).await;
                    }
                }

                Ok(())
            }
            None => {
                info!(trade_no = %trade_no, source = "shadow_worker_v2", "Transaction broadcast result is uncertain");

                if Self::is_evm_chain_code(&req.chain_code) {
                    let had_uncertain_since = req.broadcast_uncertain_since_at.is_some();
                    let rows_affected = ApiCollectRepo::mark_broadcast_uncertain_attempt(
                        &self.collect_pool,
                        trade_no,
                    )
                    .await
                    .map_err(|e| ServiceError::Database(e.into()))?;
                    let refreshed = self.get_collect_entity(trade_no).await?;
                    info!(
                        trade_no = %trade_no,
                        tx_hash = %req.tx_hash.as_deref().unwrap_or_default(),
                        nonce = req.nonce,
                        rows_affected = %rows_affected,
                        uncertain_since_at_present_before = had_uncertain_since,
                        retry_count = refreshed.broadcast_uncertain_retry_count,
                        source = "shadow_worker_v2",
                        "EVM broadcast uncertain state recorded"
                    );
                }
                Ok(())
            }
        }
    }

    // Confirm 不由 Shadow Worker 处理
    // 链上结果由 MQTT 注入，由 Domain 层落库
    // process_confirm 方法已被删除，因为它违反了职责边界

    /// 从数据库中获取归集交易信息
    pub(crate) async fn get_collect_entity(
        &self,
        trade_no: &str,
    ) -> Result<ApiCollectEntity, ServiceError> {
        let entity = ApiCollectRepo::get_api_collect_by_trade_no(&self.collect_pool, trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        Ok(entity)
    }

    /// 解析执行地址
    async fn resolve_collect_to_addr(
        &self,
        req: &ApiCollectEntity,
    ) -> Result<String, ServiceError> {
        info!(trade_no = %req.trade_no, source = "shadow_worker_v2", "Resolving collect to address");

        // 1. 根据from_addr + chain_code查询account
        let account = match wallet_database::repositories::api_wallet::account::ApiAccountRepo::find_one_by_address_chain_code(
            &req.from_addr,
            &req.chain_code,
            &self.core_pool,
        )
        .await
        {
            Ok(Some(account)) => account,
            Ok(None) => {
                error!(trade_no = %req.trade_no, from_addr = %req.from_addr, chain_code = %req.chain_code, source = "shadow_worker_v2", "Account not found");
                return Err(ServiceError::Business(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::Account(
                            crate::error::business::api_wallet::account::AccountError::NotFound,
                        ),
                    ),
                ));
            }
            Err(err) => {
                error!(trade_no = %req.trade_no, error = %err, source = "shadow_worker_v2", "Failed to find account");
                return Err(ServiceError::Database(err.into()));
            }
        };

        // 2. 查询用户归集策略
        let strategy = crate::domain::api_wallet::strategy::StrategyDomain::query_collect_strategy(
            &account.uid,
        )
        .await?;
        info!(trade_no = %req.trade_no, uid = %account.uid, source = "shadow_worker_v2", "Retrieved collect strategy");

        // 3. 根据chain_code查询链配置
        let chain_config = match strategy
            .chain_configs
            .into_iter()
            .find(|config| config.chain_code == req.chain_code)
        {
            Some(config) => config,
            None => {
                error!(trade_no = %req.trade_no, chain_code = %req.chain_code, source = "shadow_worker_v2", "Chain config not found");
                return Err(ServiceError::Business(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::ChainConfigNotFound(
                            req.chain_code.clone(),
                        ),
                    ),
                ));
            }
        };

        Ok(chain_config.normal_address.address)
    }

    /// 检查手续费是否允许继续执行
    ///
    /// 返回值语义：
    /// - Ok(true): 手续费充足，可以继续构建
    /// - Ok(false): 手续费不足，caller 必须作废当前 build 事实（invalidate_raw_tx）
    /// - Err(_): 基础设施错误
    ///
    /// ⚠️ 本方法不做任何状态/事实写入
    /// ⚠️ 不存在"等待 / 重试 / 标记"语义
    pub(crate) async fn check_fee(&self, req: &ApiCollectEntity) -> Result<bool, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 开始检查手续费, 发送方={}, 接收方={}, 金额={}, 代币地址={:?}", 
            req.from_addr, req.to_addr, req.value, req.token_addr);

        // 查询主币信息
        let chain_code: ChainCode = req.chain_code.as_str().try_into()?;
        let main_coin = ApiChainTransDomain::main_coin(&req.chain_code).await?;
        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 主币信息: 币种={}, 小数位数={}", main_coin.symbol, main_coin.decimals);

        // 确定代币信息
        let (token_symbol, token_key, token_decimals) = if req.token_addr.is_contract() {
            let token_coin =
                ApiCoinDomain::get_coin_by_token_key_exact(&req.chain_code, req.token_addr.clone())
                    .await?;
            tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 代币信息: 币种={}, 代币地址={:?}, 小数位数={}", 
                token_coin.symbol, token_coin.token_address, token_coin.decimals);
            (token_coin.symbol, token_coin.token_address, token_coin.decimals)
        } else {
            (main_coin.symbol.clone(), AssetTokenKey::Native, main_coin.decimals)
        };

        // 查询资产主币余额
        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 查询主币余额");
        let balance_str = self
            .query_balance(&req.from_addr, chain_code, AssetTokenKey::Native, main_coin.decimals)
            .await?;
        let balance = conversion::decimal_from_str(&balance_str)?;
        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 主币余额查询完成: {}", balance);

        // 估算手续费
        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 估算手续费参数: 发送方={}, 接收方={}, 金额={}, 主币={}, 代币={}, 代币小数位数={}", 
            req.from_addr, req.to_addr, req.value, main_coin.symbol, token_symbol, token_decimals);
        let spend_all_native = Self::should_spend_all_native_collect(&req.chain_code, &token_key);
        let fee_str = match self
            .estimate_fee(
                &req.from_addr,
                &req.to_addr,
                &req.value,
                chain_code,
                &token_symbol,
                &main_coin.symbol,
                token_key,
                token_decimals,
                spend_all_native,
            )
            .await
        {
            Ok(fee_str) => fee_str,
            Err(err) if Self::is_insufficient_fee_balance_error(&err) => {
                tracing::warn!(
                    trade_no = %req.trade_no,
                    from_addr = %req.from_addr,
                    to_addr = %req.to_addr,
                    chain_code = %req.chain_code,
                    token_addr = %req.token_addr,
                    source = "shadow_worker_v2",
                    "Fee estimation reported insufficient fee balance; reopening service fee cycle"
                );
                return Ok(false);
            }
            Err(err) => return Err(err),
        };
        let fee = conversion::decimal_from_str(&fee_str)?;
        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 估算手续费完成: {}", fee_str);

        // 计算需要的总金额
        let need = if spend_all_native {
            tracing::info!(
                trade_no=%req.trade_no,
                source = "shadow_worker_v2",
                "collect_tx:send: native SOL spend_all, fee check uses fee only and final amount will sweep the remaining balance"
            );
            let value = conversion::decimal_from_str(&req.value)?;
            tracing::info!(
                trade_no=%req.trade_no,
                source = "shadow_worker_v2",
                spend_all_native = true,
                requested_value = %value,
                "collect_tx:send: native SOL spend_all build branch selected"
            );
            Self::collect_balance_need(fee, value, true)?
        } else if req.token_addr.is_contract() {
            // 代币交易只需要手续费
            tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 代币交易，只需要手续费");
            fee
        } else {
            // 主币交易需要手续费+转账金额
            let value = conversion::decimal_from_str(&req.value)?;
            tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 主币交易，需要手续费+转账金额, 转账金额={}", value);
            Self::collect_balance_need(fee, value, false)?
        };

        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 手续费检查结果 - 可用余额: {}, 需要金额: {}, 手续费: {}", balance, need, fee);

        if fee > Decimal::from(0) && balance < need {
            tracing::warn!(
                trade_no = %req.trade_no,
                from_addr = %req.from_addr,
                to_addr = %req.to_addr,
                chain_code = %req.chain_code,
                token_addr = %req.token_addr,
                balance = %balance,
                need = %need,
                fee = %fee,
                stage = "build.check_fee",
                source = "shadow_worker_v2",
                "Insufficient balance for build fee check"
            );

            // 计算需要补充的手续费
            // NOTE: fee_to_upload is calculated for Fee module consumption.
            // Shadow worker must not trigger fee upload.
            let mut fee_to_upload = if let Some(f) = fee.to_f64() { f } else { 0.0 };
            if chain_code == ChainCode::Ethereum || chain_code == ChainCode::BnbSmartChain {
                fee_to_upload = fee_to_upload * 2.0;
                tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 以太坊/BSC网络，手续费翻倍: {}", fee_to_upload);
            }

            // 由 caller 进入后续的 recover / side-effect 流程处理手续费补单。
            Ok(false)
        } else {
            tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 手续费充足，继续交易");
            Ok(true)
        }
    }

    /// Fee 不足时的构建回退策略。
    ///
    /// - 首次手续费不足：继续打回 `need_service_fee = true`
    /// - fee cycle 已完成后再次构建失败：只清理 `raw_tx/tx_hash`
    ///   并保留 fee facts，避免把单子重新卡进等待手续费结果的死循环
    pub async fn invalidate_build_attempt_after_fee_check_failure(
        &self,
        req: &ApiCollectEntity,
    ) -> Result<u64, ServiceError> {
        let fee_cycle_completed = req.service_fee_uploaded_at.is_some();

        if fee_cycle_completed {
            info!(
                trade_no = %req.trade_no,
                service_fee_uploaded_at = ?req.service_fee_uploaded_at,
                tx_fee_res_ack_sent_at = ?req.tx_fee_res_ack_sent_at,
                source = "shadow_worker_v2",
                "Fee cycle already completed; using rebuild-only invalidation after fee check failure"
            );

            return ApiCollectRepo::invalidate_raw_tx_for_rebuild(
                &self.collect_pool,
                &req.trade_no,
                Some(ApiCollectStatus::InsufficientBalance),
            )
            .await
            .map_err(|e| ServiceError::Database(e.into()));
        }

        info!(
            trade_no = %req.trade_no,
            service_fee_uploaded_at = ?req.service_fee_uploaded_at,
            tx_fee_res_ack_sent_at = ?req.tx_fee_res_ack_sent_at,
            source = "shadow_worker_v2",
            "No fee-cycle facts found; reopening service fee cycle after fee check failure"
        );

        ApiCollectRepo::invalidate_raw_tx_need_service_fee(
            &self.collect_pool,
            &req.trade_no,
            Some(ApiCollectStatus::InsufficientBalance),
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))
    }

    pub(crate) async fn resolve_withdraw_from_addr(
        pool: &ApiWalletDbPool,
        req: &ApiCollectEntity,
    ) -> Result<String, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 开始解析提币地址");
        // 1. 根据from_addr + chain_code查询account
        let account = match ApiAccountRepo::find_one_by_address_chain_code(
            &req.from_addr,
            &req.chain_code,
            &pool,
        )
        .await?
        {
            Some(account) => account,
            None => {
                tracing::warn!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 提币账户不存在, from_addr={}, chain_code={}", req.from_addr, req.chain_code);
                return Err(ServiceError::Business(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::Account(
                            crate::error::business::api_wallet::account::AccountError::NotFound,
                        ),
                    ),
                ));
            }
        };

        // 2. 根据account.wallet_address查询wallet
        let wallet = match ApiWalletRepo::find_by_address(&pool.clone(), &account.wallet_address)
            .await?
        {
            Some(wallet) => wallet,
            None => {
                tracing::warn!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 钱包不存在, wallet_address={}", account.wallet_address);
                return Err(ServiceError::Business(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::Wallet(
                            crate::error::business::api_wallet::wallet::WalletError::NotFound
                                .into(),
                        ),
                    ),
                ));
            }
        };
        let Some(bind_address) = wallet.binding_address else {
            tracing::warn!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 钱包未绑定地址, wallet_address={}", account.wallet_address);
            return Err(ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::ApiWalletError::Wallet(
                        crate::error::business::api_wallet::wallet::WalletError::SubAccountWalletNotBoundWithdrawalWalletAddress
                            .into(),
                    ),
                ),
            ));
        };

        let Some(withdraw_wallet) =
            ApiWalletRepo::find_by_address(&pool.clone(), &bind_address).await?
        else {
            tracing::warn!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 出款钱包不存在, bind_address={}", bind_address);
            return Err(ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::Wallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                ),
            )));
        };

        // 3. 查询用户提币策略
        let strategy = StrategyDomain::query_withdraw_strategy(&withdraw_wallet.uid).await?;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 获取提现策略成功, 包含 {} 条链配置", strategy.chain_configs.len());

        // 4. 根据chain_code查询链配置
        let chain_config = match strategy
            .chain_configs
            .into_iter()
            .find(|config| config.chain_code == req.chain_code)
        {
            Some(config) => config,
            None => {
                tracing::error!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 未找到对应的链配置, chain_code={}", req.chain_code);
                return Err(ServiceError::Business(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::ChainConfigNotFound(
                            req.chain_code.clone(),
                        ),
                    ),
                ));
            }
        };

        // 5. 根据risk_addr决定normal/risk地址
        // risk_addr: 1 正常地址，2 风险地址
        let exec_to_addr = chain_config.normal_address.address;

        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 解析执行地址成功, exec_to_addr={}", exec_to_addr);
        Ok(exec_to_addr)
    }

    /// 查询余额
    async fn query_balance(
        &self,
        owner_address: &str,
        chain_code: ChainCode,
        token_key: AssetTokenKey,
        decimals: u8,
    ) -> Result<String, ServiceError> {
        tracing::info!(owner_address=%owner_address, chain_code=%chain_code.to_string(), token_address=%token_key.as_db_str(),
            source = "shadow_worker_v2", "collect_tx:send: 查询余额");

        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter(&chain_code.to_string()).await?;
        let balance = adapter.balance_token_key(&owner_address, token_key.clone()).await?;
        let amount = unit::format_to_string(balance, decimals)?;

        tracing::info!(owner_address=%owner_address, chain_code=%chain_code.to_string(), token_address=%token_key.as_db_str(),
            source = "shadow_worker_v2", "collect_tx:send: 查询余额完成: {}", amount);
        Ok(amount)
    }

    /// 估算手续费
    pub(crate) async fn estimate_fee(
        &self,
        from: &str,
        to: &str,
        value: &str,
        chain_code: ChainCode,
        symbol: &str,
        main_symbol: &str,
        token_key: AssetTokenKey,
        decimals: u8,
        spend_all_native: bool,
    ) -> Result<String, ServiceError> {
        // TODO: 可优化速度
        let start_time = std::time::Instant::now();
        tracing::info!(from=%from, to=%to, value=%value, chain_code=%chain_code.to_string(), symbol=%symbol,
            main_symbol=%main_symbol, token_address=%token_key.as_db_str(),
            source = "shadow_worker_v2", "collect_tx:send: 估算交易手续费开始");

        let adapter_start = std::time::Instant::now();
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter(&chain_code.to_string()).await?;
        tracing::info!(chain_code=%chain_code.to_string(), duration_ms=%adapter_start.elapsed().as_millis(), source = "shadow_worker_v2", "collect_tx:send: 获取适配器完成");

        let params_start = std::time::Instant::now();
        let mut params = ApiBaseTransferReq::new(from, to, value, &chain_code.to_string());
        params.with_token(token_key.to_chain_token_option(), decimals, symbol);
        params.spend_all = spend_all_native;
        tracing::info!(chain_code=%chain_code.to_string(), duration_ms=%params_start.elapsed().as_millis(), source = "shadow_worker_v2", "collect_tx:send: 构建请求参数完成");

        let estimate_start = std::time::Instant::now();
        let fee = adapter.estimate_fee(params, main_symbol).await?;
        tracing::info!(chain_code=%chain_code.to_string(), duration_ms=%estimate_start.elapsed().as_millis(), source = "shadow_worker_v2", "collect_tx:send: 调用estimate_fee完成");

        let parse_start = std::time::Instant::now();
        let amount = match chain_code {
            ChainCode::Tron => {
                let res: TronFeeDetails = wallet_utils::serde_func::serde_from_str(&fee)?;
                res.estimate_fee.amount.to_string()
            }
            ChainCode::Bitcoin => todo!(),
            ChainCode::Solana => {
                let res: CommonFeeDetails = wallet_utils::serde_func::serde_from_str(&fee)?;
                res.estimate_fee.amount.to_string()
            }
            ChainCode::Ethereum => {
                let res: FeeDetailsVo<EthereumFeeDetails> =
                    wallet_utils::serde_func::serde_from_str(&fee)?;
                let mut amount: f64 = 0.0;
                for it in res.data {
                    amount = amount + it.estimate_fee.amount;
                }
                amount.to_string()
            }
            ChainCode::BnbSmartChain => {
                let res: FeeDetailsVo<EthereumFeeDetails> =
                    wallet_utils::serde_func::serde_from_str(&fee)?;
                let mut amount: f64 = 0.0;
                for it in res.data {
                    amount = amount + it.estimate_fee.amount;
                }
                amount.to_string()
            }
            ChainCode::Litecoin => todo!(),
            ChainCode::Dogcoin => todo!(),
            ChainCode::Sui => todo!(),
            ChainCode::Ton => todo!(),
        };
        tracing::info!(chain_code=%chain_code.to_string(), duration_ms=%parse_start.elapsed().as_millis(), source = "shadow_worker_v2", "collect_tx:send: 解析手续费结果完成");

        tracing::info!(from=%from, to=%to, chain_code=%chain_code.to_string(), total_duration_ms=%start_time.elapsed().as_millis(), source = "shadow_worker_v2", "collect_tx:send: 估算手续费完成: {}", amount);
        Ok(amount)
    }

    /// 获取归集配置
    async fn get_collect_config(
        &self,
        uid: &str,
        chain_code: &str,
    ) -> Result<ChainConfig, ServiceError> {
        tracing::info!(uid=%uid, chain_code=%chain_code, source = "shadow_worker_v2", "collect_tx:send: 查询归集策略");

        // 查询策略
        let strategy = StrategyDomain::query_collect_strategy(uid).await?;

        tracing::info!(uid=%uid, source = "shadow_worker_v2", "collect_tx:send: 获取归集策略成功，包含 {} 条链配置", strategy.chain_configs.len());

        let Some(chain_config) =
            strategy.chain_configs.into_iter().find(|config| config.chain_code == chain_code)
        else {
            tracing::error!(uid=%uid, chain_code=%chain_code, source = "shadow_worker_v2", "collect_tx:send: 未找到对应的链配置");
            return Err(crate::error::business::BusinessError::ApiWallet(
                ApiWalletError::ChainConfigNotFound(chain_code.to_owned()),
            )
            .into());
        };

        tracing::info!(uid=%uid, chain_code=%chain_code, source = "shadow_worker_v2", "collect_tx:send: 找到链配置, normal_address={}", chain_config.normal_address.address);
        Ok(chain_config)
    }
    async fn check_digest(&self, req: &ApiCollectEntity) -> Result<bool, ServiceError> {
        info!(trade_no = %req.trade_no, source = "shadow_worker_v2", "Checking transaction digest");

        let sn = crate::context::get_context().unwrap().get_sn();
        let mut d = wallet_utils::conversion::decimal_from_str(req.value.as_str())?;
        d = d.normalize();
        // ⚠️ 这里必须用后端给的空字符串的to_addr，不能用查询策略解析的地址
        let raw_data = req.from_addr.clone() + "" + d.to_string().as_str() + sn;
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(&raw_data));

        let is_valid = req.validate == digest;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 交易摘要验证完成, 结果: {}", is_valid);
        Ok(is_valid)
    }

    /// 生成转账请求
    ///
    /// ⚠️ nonce is a FACT decided in Phase 1.
    /// gen_transfer_req MUST NOT:
    /// - compute nonce
    /// - fallback nonce
    /// - modify nonce semantics
    async fn gen_transfer_req(
        &self,
        req: &ApiCollectEntity,
        exec_to_addr: &str,
        nonce: u64, // 外部传入的nonce
    ) -> Result<crate::request::api_wallet::trans::ApiTransferReq, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 开始生成转账请求, exec_to_addr={}, nonce={}", exec_to_addr, nonce);

        // 获取币种信息
        let coin = ApiCoinDomain::get_coin_by_token_key_exact(
            &req.chain_code,
            req.token_addr.clone().into(),
        )
        .await?;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 获取币种信息成功, symbol={}, token_address={:?}, decimals={}", 
            coin.symbol, coin.token_address, coin.decimals);

        // 创建基础转账请求 - 使用exec_to_addr而非req.to_addr
        let mut params =
            ApiBaseTransferReq::new(&req.from_addr, exec_to_addr, &req.value, &req.chain_code);
        let token_address = if coin.token_address.is_native() {
            None
        } else {
            let s = coin.token_address.as_db_str().to_string();
            if s.is_empty() { None } else { Some(s) }
        };
        params.with_token(token_address, coin.decimals, &coin.symbol);
        params.spend_all =
            Self::should_spend_all_native_collect(&req.chain_code, &coin.token_address);
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 创建基础转账请求成功");

        // 获取钱包解锁态 token
        let unlock_token = ApiWalletDomain::get_wallet_unlock_token().await?;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 获取钱包解锁态成功");

        let transfer_req = ApiTransferReq { base: params, password: unlock_token, nonce };
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 生成转账请求成功");
        Ok(transfer_req)
    }

    /// Check if a chain is EVM-compatible
    ///
    /// EVM chains require nonce management for transaction ordering.
    /// This function centralizes the EVM chain detection to avoid
    /// scattered match statements and ensure consistency.
    fn is_evm_chain(chain: &ChainCode) -> bool {
        matches!(chain, ChainCode::Ethereum | ChainCode::BnbSmartChain)
    }

    /// Allocate nonce for transaction.
    ///
    /// ⚠️ NONCE INVARIANT:
    /// - EVM nonce is an irreversible, monotonic fact once allocated.
    /// - Once this function returns, the nonce is considered CONSUMED,
    ///   regardless of whether build/broadcast succeeds or fails.
    /// - Nonce MUST NOT be rolled back under any circumstances.
    /// - Any retry MUST allocate a NEW nonce.
    ///
    /// This matches EVM semantics:
    /// nonce = count of sent (confirmed OR pending) transactions.
    ///
    /// ⚠️ DO NOT:
    /// - read nonce then +1
    /// - fallback to chain nonce
    /// - attempt to "reuse" nonce on failure
    ///
    /// Violating any of the above will cause nonce duplication
    /// under concurrency or restart scenarios.
    async fn get_nonce(&self, from_addr: &str, chain_code: &str) -> Result<u64, ServiceError> {
        info!(from_addr = %from_addr, chain_code = %chain_code, source = "shadow_worker_v2", "Getting nonce");
        let chain: ChainCode = chain_code.try_into()?;

        // ⚠️ EVM nonce MUST be allocated via DB atomic upsert.
        // Any read-modify-write logic here is forbidden.
        match chain {
            c if Self::is_evm_chain(&c) => {
                // 对于以太坊类链，使用NonceEngine来分配nonce
                // ⚠️ INVARIANT:
                // This method MUST guarantee DB-level atomic CAS for nonce allocation.
                // Any refactor breaking this invariant will cause nonce duplication.

                let nonce_engine = get_nonce_engine();
                let nonce =
                    nonce_engine.allocate_nonce(from_addr, chain_code, &self.collect_pool).await?;
                info!(from_addr = %from_addr, chain_code = %chain_code, nonce = %nonce, source = "shadow_worker_v2", "Retrieved nonce using NonceEngine");
                Ok(nonce as u64)
            }
            _ => {
                // 非 EVM 链不参与 nonce 分配
                Ok(0)
            }
        }
    }

    /// 交易恢复逻辑 - 处理已有tx_hash的交易
    ///
    /// ⚠️ IMPORTANT:
    /// - Recover logic MUST only be triggered by Scanner commands
    /// - This method should NOT be called directly by other components
    /// - On-chain confirmation fact is owned by Scanner / Shadow Recovery ONLY
    async fn recover_tx(
        &self,
        req: &ApiCollectEntity,
    ) -> Result<Option<crate::domain::chain::TransferResp>, ServiceError> {
        let tx_hash = req.tx_hash.as_ref().unwrap();
        info!(trade_no = %req.trade_no, tx_hash = %tx_hash, source = "shadow_worker_v2", "Processing recovered tx");

        match crate::domain::api_wallet::trans::ApiTransDomain::process_recovered_tx(
            &req.chain_code,
            &req.from_addr,
            tx_hash,
            req.nonce,
            &req.transaction_fee,
        )
        .await
        {
            Ok(Some(tx_resp)) => {
                info!(trade_no = %req.trade_no, tx_hash = %tx_hash, source = "shadow_worker_v2", "Recovered tx success");
                Ok(Some(tx_resp))
            }
            Ok(None) => {
                info!(trade_no = %req.trade_no, tx_hash = %tx_hash, source = "shadow_worker_v2", "Recovered tx result is uncertain, will retry");
                Ok(None)
            }
            Err(err) => {
                error!(trade_no = %req.trade_no, tx_hash = %tx_hash, error = %err, source = "shadow_worker_v2", "Recovered tx failed");
                Err(err)
            }
        }
    }

    /// 处理归集交易失败
    async fn handle_collect_tx_failed(
        &self,
        trade_no: &str,
        err: ServiceError,
    ) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, error = %err, source = "shadow_worker_v2", "Handling collect tx failed");

        // 🔒 事实保护：检查是否已存在成功事实
        // 规则：一旦成功事实（transaction_time）成立，失败事实永远不能覆盖它
        // 这是事实系统的"单调性约束"
        let req = self.get_collect_entity(trade_no).await?;
        if req.transaction_time.is_some() {
            info!(
                trade_no = %trade_no,
                source = "shadow_worker_v2",
                "Skip mark failed: transaction already confirmed (monotonicity constraint)"
            );
            return Ok(());
        }

        // 🔒 事实保护：检查是否已被 invalidate_raw_tx 作废
        // 规则：一旦 build 事实被作废（need_service_fee = true），失败事实不能覆盖它
        // 这确保 invalidate_raw_tx 写入的错误上下文是"最终解释权"
        // NOTE:
        // need_service_fee = true represents a final build invalidation fact.
        // Failure here MUST NOT override it.
        if req.need_service_fee == Some(true) {
            info!(
                trade_no = %trade_no,
                source = "shadow_worker_v2",
                "Skip mark failed: build already invalidated (fact rollback already applied)"
            );
            return Ok(());
        }

        if Self::is_solana_rent_exempt_reserve_balance_error(&req, &err) {
            info!(
                trade_no = %trade_no,
                error = %err,
                source = "shadow_worker_v2",
                "Detected Solana rent-exempt reserve shortage; reopening service fee cycle"
            );

            let affected = self.invalidate_build_attempt_after_fee_check_failure(&req).await?;

            if affected == 0 {
                info!(
                    trade_no = %trade_no,
                    source = "shadow_worker_v2",
                    "Transaction already invalidated or no raw_tx to invalidate, skip"
                );
                self.clear_build_slot_after_claim(trade_no).await?;
            } else {
                self.advancer.try_advance(&req.trade_no).await;
            }

            return Ok(());
        }

        // BuildTx 失败只需要释放 build slot，让 scanner 后续重试。
        // 这里不写失败事实，避免把可重试的构建失败误记成终态失败。
        if self.handle_build_tx_failure_without_raw_tx(trade_no, &req).await? {
            return Ok(());
        }

        // Solana 广播常见可恢复错误：
        // - blockhash 过期（Blockhash not found）
        // 这类错误不应直接写失败事实，而应作废 raw_tx/tx_hash 触发重建。
        if req.chain_code.eq_ignore_ascii_case("sol")
            && crate::domain::api_wallet::trans::ApiTransDomain::is_blockhash_not_found_error(&err)
            && req.raw_tx.is_some()
            && req.tx_hash.is_some()
            && req.last_broadcast_at.is_none()
        {
            let rows_affected = ApiCollectRepo::invalidate_raw_tx_for_rebuild(
                &self.collect_pool,
                trade_no,
                None,
            )
            .await
            .map_err(|db_err: wallet_database::Error| {
                error!(trade_no = %trade_no, error = %db_err, source = "shadow_worker_v2", "Failed to invalidate raw_tx for sol blockhash rebuild");
                ServiceError::Database(db_err.into())
            })?;

            info!(
                trade_no = %trade_no,
                rows_affected = %rows_affected,
                error = %err,
                source = "shadow_worker_v2",
                "SOL blockhash not found detected, invalidated raw_tx for rebuild"
            );

            if rows_affected > 0 {
                self.advancer.try_advance(trade_no).await;
            }
            return Ok(());
        }

        // "already exists" 表示节点已接收广播，作为幂等成功兜底处理。
        // 避免误写 SendingTxFailed 并冻结到错误分支。
        if crate::domain::api_wallet::trans::ApiTransDomain::is_duplicate_broadcast_error(&err)
            && req.raw_tx.is_some()
            && req.tx_hash.is_some()
        {
            let rows_affected =
                wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_broadcast_executed(
                    &self.collect_pool,
                    trade_no,
                )
                .await
                .map_err(|db_err: wallet_database::Error| {
                    error!(trade_no = %trade_no, error = %db_err, source = "shadow_worker_v2", "Failed to mark broadcast executed for duplicate broadcast");
                    ServiceError::Database(db_err.into())
                })?;
            info!(
                trade_no = %trade_no,
                rows_affected = %rows_affected,
                error = %err,
                source = "shadow_worker_v2",
                "Duplicate broadcast detected, treat as idempotent success and continue"
            );
            self.advancer.try_advance(trade_no).await;
            return Ok(());
        }

        let error_msg = format!("{}", err);
        if ApiTransDomain::should_treat_nonce_too_low_as_broadcast_success(
            &req.chain_code,
            &err,
            req.raw_tx.is_some(),
            req.tx_hash.is_some(),
        ) {
            info!(
                trade_no = %trade_no,
                source = "shadow_worker_v2",
                "Detected EVM nonce too low on broadcast, treat as idempotent success"
            );

            let nonce_engine = get_nonce_engine();
            if let Err(e) =
                nonce_engine.handle_nonce_error(&req.from_addr, &req.chain_code, &error_msg).await
            {
                warn!(trade_no = %trade_no, error = %e, source = "shadow_worker_v2", "Nonce self-heal failed");
            }

            let rows_affected =
                wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_broadcast_executed(
                    &self.collect_pool,
                    trade_no,
                )
                .await
                .map_err(|db_err: wallet_database::Error| {
                    error!(trade_no = %trade_no, error = %db_err, source = "shadow_worker_v2", "Failed to mark broadcast executed for nonce too low broadcast conflict");
                    ServiceError::Database(db_err.into())
                })?;

            info!(
                trade_no = %trade_no,
                rows_affected = %rows_affected,
                error = %err,
                source = "shadow_worker_v2",
                "Nonce too low broadcast conflict treated as idempotent success"
            );

            self.advancer.try_advance(trade_no).await;
            return Ok(());
        }

        // 处理 nonce too low 错误
        if ApiTransDomain::is_nonce_too_low_error(&err) {
            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Detected nonce too low error, syncing nonce from chain");

            let nonce_engine = get_nonce_engine();
            if let Err(e) =
                nonce_engine.handle_nonce_error(&req.from_addr, &req.chain_code, &error_msg).await
            {
                warn!(trade_no = %trade_no, error = %e, source = "shadow_worker_v2", "Nonce self-heal failed");
            }

            let rows_affected = ApiCollectRepo::update_api_collect_status_and_err(
                &self.collect_pool,
                trade_no,
                wallet_database::entities::api_collect::ApiCollectStatus::SendingTxFailed,
                ErrCode::SDKInternalError, // 使用通用错误码
                &error_msg,
            )
            .await
            .map_err(|db_err: wallet_database::Error| {
                error!(trade_no = %trade_no, error = %db_err, source = "shadow_worker_v2", "Failed to update status to failed");
                ServiceError::Database(db_err.into())
            })?;

            info!(trade_no = %trade_no, rows_affected = %rows_affected, source = "shadow_worker_v2", "Updated status to failed with nonce error");

            // 只有第一次写入失败事实才发送 Tick
            if rows_affected > 0 {
                // 直接调用 try_advance 进行点对点唤醒
                self.advancer.try_advance(&trade_no).await;
            }

            return Ok(());
        }

        // 更新数据库状态为失败
        match err.retry_policy() {
            wallet_utils::error::RetryPolicy::Never => {
                let err_code = if err.is_network_error() {
                    ErrCode::NetworkException
                } else {
                    ErrCode::SDKInternalError
                };

                let rows_affected = ApiCollectRepo::update_api_collect_status_and_err(
                    &self.collect_pool,
                    trade_no,
                    wallet_database::entities::api_collect::ApiCollectStatus::SendingTxFailed,
                    err_code, // err_code - 通用失败码
                    &error_msg,
                )
                .await
                .map_err(|db_err: wallet_database::Error| {
                    error!(trade_no = %trade_no, error = %db_err, source = "shadow_worker_v2", "Failed to update status to failed");
                    ServiceError::Database(db_err.into())
                })?;
                info!(trade_no = %trade_no, rows_affected = %rows_affected, source = "shadow_worker_v2", "Updated status to failed");

                // 只有第一次写入失败事实才发送 Tick
                if rows_affected > 0 {
                    // 直接调用 try_advance 进行点对点唤醒
                    self.advancer.try_advance(&trade_no).await;
                }

                // 注意：Shadow Worker 是执行者，不是裁决者
                // 不设置 finished_at，因为链上事实尚未闭环
                // 只有 Scanner/Shadow Recovery 才能设置终态
            }
            wallet_utils::error::RetryPolicy::Delay => {
                tracing::info!(trade_no = %trade_no, error = %err, source = "shadow_worker_v2", "Collect tx failed, will retry later");
            }
        }

        Ok(())
    }

    async fn handle_build_tx_failure_without_raw_tx(
        &self,
        trade_no: &str,
        req: &ApiCollectEntity,
    ) -> Result<bool, ServiceError> {
        if req.raw_tx.is_some() {
            return Ok(false);
        }

        let rows_affected = ApiCollectRepo::clear_building_at(&self.collect_pool, trade_no)
            .await
            .map_err(|e: wallet_database::Error| {
                error!(trade_no = %trade_no, error = %e, source = "shadow_worker_v2", "Failed to clear build slot for BuildTx failure");
                ServiceError::Database(e.into())
            })?;

        info!(
            trade_no = %trade_no,
            rows_affected = %rows_affected,
            source = "shadow_worker_v2",
            "BuildTx failure cleared build slot"
        );

        Ok(true)
    }

    async fn clear_build_slot_after_claim(&self, trade_no: &str) -> Result<(), ServiceError> {
        let rows_affected = ApiCollectRepo::clear_building_at(&self.collect_pool, trade_no)
            .await
            .map_err(|e: wallet_database::Error| {
                error!(trade_no = %trade_no, error = %e, source = "shadow_worker_v2", "Failed to clear build slot after BuildTx early exit");
                ServiceError::Database(e.into())
            })?;

        info!(
            trade_no = %trade_no,
            rows_affected = %rows_affected,
            source = "shadow_worker_v2",
            "BuildTx early exit cleared build slot"
        );

        Ok(())
    }

    async fn reclaim_stale_build_slot(
        &self,
        trade_no: &str,
        building_at: Option<DateTime<Utc>>,
    ) -> Result<bool, ServiceError> {
        if !Self::is_stale_build_slot(building_at, Utc::now()) {
            return Ok(false);
        }

        let rows_affected = ApiCollectRepo::clear_building_at(&self.collect_pool, trade_no)
            .await
            .map_err(|e: wallet_database::Error| {
                error!(trade_no = %trade_no, error = %e, source = "shadow_worker_v2", "Failed to clear stale build slot");
                ServiceError::Database(e.into())
            })?;

        info!(
            trade_no = %trade_no,
            rows_affected = %rows_affected,
            building_at = ?building_at,
            source = "shadow_worker_v2",
            "Cleared stale build slot before retrying BuildTx"
        );

        Ok(rows_affected > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::ShadowCollectWorker;
    use chrono::Utc;
    use rust_decimal::Decimal;
    use std::{str::FromStr, sync::Arc};
    use tempfile::tempdir;
    use tokio::sync::mpsc;
    use wallet_database::{
        ApiWalletDbPool, SqliteContext,
        entities::{
            api_collect::{ApiCollectEntity, ApiCollectStatus},
            asset_token_key::AssetTokenKey,
        },
        repositories::api_wallet::collect::ApiCollectRepo,
    };

    fn base_collect() -> ApiCollectEntity {
        ApiCollectEntity {
            id: 1,
            name: "collect".to_string(),
            uid: "uid".to_string(),
            from_addr: "from".to_string(),
            to_addr: "old-to".to_string(),
            value: "1.12".to_string(),
            validate: "digest".to_string(),
            chain_code: "sol".to_string(),
            token_addr: AssetTokenKey::Contract("token".to_string()),
            symbol: "USDC".to_string(),
            trade_no: "trade-no".to_string(),
            trade_type: 2,
            risk_addr: 1,
            status: ApiCollectStatus::Init,
            nonce: 0,
            tx_hash: None,
            transaction_fee: "0".to_string(),
            transaction_time: None,
            block_height: Some("0".to_string()),
            notes: Some(String::new()),
            post_tx_count: 0,
            post_confirm_tx_count: 0,
            err_code: None,
            err_msg: Some(String::new()),
            order_ack_sent_at: Some(Utc::now()),
            raw_tx: None,
            resource_consume: "0".to_string(),
            building_at: None,
            last_broadcast_at: None,
            broadcast_uncertain_since_at: None,
            broadcast_uncertain_retry_count: 0,
            broadcast_uncertain_last_checked_at: None,
            broadcast_uncertain_reconciled_at: None,
            broadcast_uncertain_rebroadcast_count: 0,
            result_ack_sent_at: None,
            result_ack_send_count: 0,
            tx_res_received_at: None,
            service_fee_order_received_at: None,
            service_fee_uploaded_at: None,
            need_service_fee: None,
            ever_needed_service_fee: false,
            tx_fee_res_ack_sent_at: None,
            tx_exec_receipt_uploaded_at: None,
            finished_at: None,
            created_at: Utc::now(),
            updated_at: Some(Utc::now()),
        }
    }

    #[tokio::test]
    async fn clear_build_slot_after_claim_releases_building_at() {
        let dir = tempdir().expect("tempdir");
        let dir_path = dir.path().to_string_lossy().to_string();

        let collect_ctx = SqliteContext::new(&dir_path, Some("api_transaction.db"))
            .await
            .expect("init api_transaction.db");
        let collect_pool = collect_ctx.into_transaction_db_pool().expect("transaction pool");

        let wallet_ctx =
            SqliteContext::new(&dir_path, Some("api_wallet.db")).await.expect("init api_wallet.db");
        let wallet_pool: ApiWalletDbPool =
            wallet_ctx.into_api_wallet_db_pool().expect("wallet pool");

        let (intent_tx, _intent_rx) = mpsc::channel(1);
        let worker = ShadowCollectWorker::new(
            collect_pool.clone(),
            wallet_pool,
            Arc::new(crate::infrastructure::api_trans::collect::legacy::AddressLockManager::new()),
            Arc::new(crate::infrastructure::api_trans::collect::shadow::ShadowAdvancer::new(
                collect_pool.clone(),
                intent_tx,
                None,
            )),
        );

        let trade_no = "T_clear_build_slot_after_claim";
        ApiCollectRepo::upsert_api_collect(
            &collect_pool,
            "uid",
            "collect",
            "from",
            "to",
            "1.12",
            "digest",
            "sol",
            None,
            "USDC",
            trade_no,
            2,
            ApiCollectStatus::Init,
            1,
        )
        .await
        .expect("insert collect");

        let claimed = ApiCollectRepo::update_building_at(&collect_pool, trade_no)
            .await
            .expect("claim build slot");
        assert_eq!(claimed, 1);

        worker.clear_build_slot_after_claim(trade_no).await.expect("clear build slot");

        let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, trade_no)
            .await
            .expect("load collect");
        assert!(persisted.building_at.is_none());
    }

    #[tokio::test]
    async fn reclaim_stale_build_slot_clears_and_allows_reclaim() {
        let dir = tempdir().expect("tempdir");
        let dir_path = dir.path().to_string_lossy().to_string();

        let collect_ctx = SqliteContext::new(&dir_path, Some("api_transaction.db"))
            .await
            .expect("init api_transaction.db");
        let collect_pool = collect_ctx.into_transaction_db_pool().expect("transaction pool");

        let wallet_ctx =
            SqliteContext::new(&dir_path, Some("api_wallet.db")).await.expect("init api_wallet.db");
        let wallet_pool: ApiWalletDbPool =
            wallet_ctx.into_api_wallet_db_pool().expect("wallet pool");

        let (intent_tx, _intent_rx) = mpsc::channel(1);
        let worker = ShadowCollectWorker::new(
            collect_pool.clone(),
            wallet_pool,
            Arc::new(crate::infrastructure::api_trans::collect::legacy::AddressLockManager::new()),
            Arc::new(crate::infrastructure::api_trans::collect::shadow::ShadowAdvancer::new(
                collect_pool.clone(),
                intent_tx,
                None,
            )),
        );

        let trade_no = "T_reclaim_stale_build_slot";
        ApiCollectRepo::upsert_api_collect(
            &collect_pool,
            "uid",
            "collect",
            "from",
            "to",
            "1.12",
            "digest",
            "sol",
            None,
            "USDC",
            trade_no,
            2,
            ApiCollectStatus::Init,
            1,
        )
        .await
        .expect("insert collect");
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 building_at = datetime('now', '-31 seconds')
             WHERE trade_no = ?",
        )
        .bind(trade_no)
        .execute(collect_pool.as_ref())
        .await
        .expect("seed stale slot");

        let req = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, trade_no)
            .await
            .expect("load collect");
        assert_eq!(
            req.building_at.map(|ts| Utc::now().signed_duration_since(ts).num_seconds() >= 30),
            Some(true)
        );

        let reclaimed = worker
            .reclaim_stale_build_slot(trade_no, req.building_at)
            .await
            .expect("reclaim stale slot");
        assert!(reclaimed);

        let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, trade_no)
            .await
            .expect("load collect");
        assert!(persisted.building_at.is_none());

        let claimed = ApiCollectRepo::update_building_at(&collect_pool, trade_no)
            .await
            .expect("reclaim build slot");
        assert_eq!(claimed, 1);
    }

    #[tokio::test]
    async fn fresh_build_slot_is_not_reclaimed() {
        let dir = tempdir().expect("tempdir");
        let dir_path = dir.path().to_string_lossy().to_string();

        let collect_ctx = SqliteContext::new(&dir_path, Some("api_transaction.db"))
            .await
            .expect("init api_transaction.db");
        let collect_pool = collect_ctx.into_transaction_db_pool().expect("transaction pool");

        let wallet_ctx =
            SqliteContext::new(&dir_path, Some("api_wallet.db")).await.expect("init api_wallet.db");
        let wallet_pool: ApiWalletDbPool =
            wallet_ctx.into_api_wallet_db_pool().expect("wallet pool");

        let (intent_tx, _intent_rx) = mpsc::channel(1);
        let worker = ShadowCollectWorker::new(
            collect_pool.clone(),
            wallet_pool,
            Arc::new(crate::infrastructure::api_trans::collect::legacy::AddressLockManager::new()),
            Arc::new(crate::infrastructure::api_trans::collect::shadow::ShadowAdvancer::new(
                collect_pool.clone(),
                intent_tx,
                None,
            )),
        );

        let trade_no = "T_fresh_build_slot";
        ApiCollectRepo::upsert_api_collect(
            &collect_pool,
            "uid",
            "collect",
            "from",
            "to",
            "1.12",
            "digest",
            "sol",
            None,
            "USDC",
            trade_no,
            2,
            ApiCollectStatus::Init,
            1,
        )
        .await
        .expect("insert collect");
        sqlx::query(
            "UPDATE api_collect
             SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'),
                 building_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE trade_no = ?",
        )
        .bind(trade_no)
        .execute(collect_pool.as_ref())
        .await
        .expect("seed fresh slot");

        let req = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, trade_no)
            .await
            .expect("load collect");
        let reclaimed = worker
            .reclaim_stale_build_slot(trade_no, req.building_at)
            .await
            .expect("reclaim fresh slot");
        assert!(!reclaimed);

        let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, trade_no)
            .await
            .expect("load collect");
        assert!(persisted.building_at.is_some());

        let claimed = ApiCollectRepo::update_building_at(&collect_pool, trade_no)
            .await
            .expect("claim build slot");
        assert_eq!(claimed, 0);
    }

    #[test]
    fn collect_rebuild_refreshes_to_addr() {
        let mut req = base_collect();

        let changed = ShadowCollectWorker::apply_exec_to_addr(&mut req, "new-to");

        assert!(changed);
        assert_eq!(req.to_addr, "new-to");
    }

    #[test]
    fn collect_rebuild_keeps_to_addr_when_already_latest() {
        let mut req = base_collect();
        req.to_addr = "same-to".to_string();

        let changed = ShadowCollectWorker::apply_exec_to_addr(&mut req, "same-to");

        assert!(!changed);
        assert_eq!(req.to_addr, "same-to");
    }

    #[test]
    fn recover_skip_nudges_advance_when_chain_fact_exists() {
        let mut req = base_collect();
        req.transaction_time = Some(Utc::now());
        req.tx_exec_receipt_uploaded_at = None;

        assert!(ShadowCollectWorker::should_nudge_advance_after_recover_skip(&req));
    }

    #[test]
    fn recover_skip_does_not_nudge_without_chain_fact() {
        let req = base_collect();

        assert!(!ShadowCollectWorker::should_nudge_advance_after_recover_skip(&req));
    }

    #[test]
    fn recover_hash_validation_accepts_missing_existing_hash() {
        ShadowCollectWorker::validate_recovered_tx_hash("trade-no", None, "0xrecover")
            .expect("missing local hash should be accepted for backfill");
    }

    #[test]
    fn recover_hash_validation_rejects_mismatch() {
        let err = ShadowCollectWorker::validate_recovered_tx_hash(
            "trade-no",
            Some("0xlocal"),
            "0xrecover",
        )
        .expect_err("mismatched hash must fail");

        assert!(err.to_string().contains("recover tx_hash mismatch"));
    }

    #[test]
    fn native_sol_collect_uses_spend_all() {
        assert!(ShadowCollectWorker::should_spend_all_native_collect(
            "sol",
            &AssetTokenKey::Native
        ));
        assert!(!ShadowCollectWorker::should_spend_all_native_collect(
            "eth",
            &AssetTokenKey::Native
        ));
        assert!(!ShadowCollectWorker::should_spend_all_native_collect(
            "sol",
            &AssetTokenKey::Contract("token".to_string())
        ));
    }

    #[test]
    fn native_sol_spend_all_fee_check_uses_fee_only() {
        let fee = Decimal::from_str("0.000005").expect("fee");
        let value = Decimal::from_str("0.01299088").expect("value");
        let need =
            ShadowCollectWorker::collect_balance_need(fee, value, true).expect("spend-all need");
        assert_eq!(need, fee);
    }

    #[test]
    fn non_spend_all_fee_check_keeps_requested_value() {
        let fee = Decimal::from_str("0.000005").expect("fee");
        let value = Decimal::from_str("0.01299088").expect("value");
        let need = ShadowCollectWorker::collect_balance_need(fee, value, false)
            .expect("non spend-all need");

        assert_eq!(need, fee + value);
    }

    #[test]
    fn sol_rent_exempt_reserve_balance_error_reopens_fee_cycle() {
        use crate::error::{
            business::{
                BusinessError,
                chain::{ChainError, InsufficientBalanceDetail},
            },
            service::ServiceError,
        };

        let req = base_collect();
        let err = ServiceError::Business(BusinessError::Chain(ChainError::InsufficientBalance(
            InsufficientBalanceDetail::new()
                .reason("sender balance must keep rent-exempt reserve after transfer"),
        )));

        assert!(ShadowCollectWorker::is_solana_rent_exempt_reserve_balance_error(&req, &err));
    }

    #[test]
    fn non_sol_chain_does_not_reopen_fee_cycle_for_rent_reserve_error() {
        use crate::error::{
            business::{
                BusinessError,
                chain::{ChainError, InsufficientBalanceDetail},
            },
            service::ServiceError,
        };

        let mut req = base_collect();
        req.chain_code = "eth".to_string();
        let err = ServiceError::Business(BusinessError::Chain(ChainError::InsufficientBalance(
            InsufficientBalanceDetail::new()
                .reason("sender balance must keep rent-exempt reserve after transfer"),
        )));

        assert!(!ShadowCollectWorker::is_solana_rent_exempt_reserve_balance_error(&req, &err));
    }
}
