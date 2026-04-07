// withdraw/shadow/worker/shadow_withdraw_worker.rs
#![allow(deprecated)]

use std::sync::Arc;

use chrono::Utc;
use tracing::{error, info, warn};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool,
    entities::api_withdraw::{ApiWithdrawEntity, ErrCode, WithdrawFailureStage},
    repositories::api_wallet::withdraw::ApiWithdrawRepo,
};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::RetryableError as _;

use crate::{
    domain::api_wallet::{
        adapter::tx::RawTx, coin::ApiCoinDomain, trans::ApiTransDomain, wallet::ApiWalletDomain,
    },
    error::{
        business::api_wallet::{ApiWalletError, trans::TransError},
        service::ServiceError,
        system::SystemError,
    },
    infrastructure::{
        api_trans::withdraw::shadow::ShadowScanner,
        nonce::nonce_engine::{ReconcileReason, get_nonce_engine},
    },
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};

/// ShadowWithdrawWorker
///
/// 负责处理链相关操作：
/// - 构建交易
/// - 广播交易
///
/// ShadowWithdrawWorker design invariant:
///
/// Phase 1: Concurrent arbitration (no network)
/// - 进行并发裁决
/// - 分配 nonce（确保同一地址串行）
/// - 禁止任何网络调用、sleep、await RPC
/// - 裁决依据必须基于 fresh read
///
/// Phase 2: Network execution (no shared state)
/// - 执行网络/RPC/构建/广播
/// - chain_rpc_guard 只限制外部世界并发
/// - 允许失败和重试
///
/// Phase 3: Irreversible fact commit
/// - 提交不可逆事实
/// - 更新数据库状态
/// - 唤醒扫描器
///
/// 🔒 核心原则：
/// - nonce 从"动态信息"升级为"已裁决事实"
/// - chain_rpc_guard 作为 RPC 压力阀
pub struct ShadowWithdrawWorker {
    pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
    /// ShadowScanner 引用，用于直接调用 try_advance
    scanner: Arc<ShadowScanner>,
}

impl ShadowWithdrawWorker {
    const TRON_RAW_EXPIRY_GUARD_MS: i64 = 3_000;
    const TRON_MISSING_CONFIRMED_AND_PENDING_MANUAL_REVIEW_TIMEOUT_SECS: i64 = 5 * 60;
    const EVM_UNCERTAIN_TIMEOUT_SECS: i64 = 5 * 60;
    // const EVM_UNCERTAIN_TIMEOUT_SECS: i64 = 20;
    const EVM_UNCERTAIN_MANUAL_REVIEW_TIMEOUT_SECS: i64 = 24 * 60 * 60;
    const EVM_UNCERTAIN_BACKOFF_MID_SECS: i64 = 15;
    // const EVM_UNCERTAIN_BACKOFF_MID_SECS: i64 = 2;
    const EVM_UNCERTAIN_BACKOFF_MAX_SECS: i64 = 30;
    // const EVM_UNCERTAIN_BACKOFF_MAX_SECS: i64 = 3;
    // 测试开关（仅用于本地压测 withdraw uncertain 超时收口）
    // 需要测试超时分支时改为 true，测完务必改回 false。
    const TEST_FORCE_WITHDRAW_EVM_RECOVER_NONE: bool = false;
    // const TEST_FORCE_WITHDRAW_EVM_RECOVER_NONE: bool = true;
    // 可选测试覆盖（None 表示使用上面的默认常量）
    const TEST_EVM_UNCERTAIN_TIMEOUT_SECS_OVERRIDE: Option<i64> = None;
    const TEST_EVM_UNCERTAIN_BACKOFF_MID_SECS_OVERRIDE: Option<i64> = None;
    const TEST_EVM_UNCERTAIN_BACKOFF_MAX_SECS_OVERRIDE: Option<i64> = None;

    fn test_force_withdraw_evm_recover_none() -> bool {
        Self::TEST_FORCE_WITHDRAW_EVM_RECOVER_NONE
    }

    fn evm_uncertain_timeout_secs() -> i64 {
        Self::TEST_EVM_UNCERTAIN_TIMEOUT_SECS_OVERRIDE
            .unwrap_or(Self::EVM_UNCERTAIN_TIMEOUT_SECS)
            .max(1)
    }

    fn evm_uncertain_backoff_mid_secs() -> i64 {
        Self::TEST_EVM_UNCERTAIN_BACKOFF_MID_SECS_OVERRIDE
            .unwrap_or(Self::EVM_UNCERTAIN_BACKOFF_MID_SECS)
            .max(0)
    }

    fn evm_uncertain_backoff_max_secs() -> i64 {
        Self::TEST_EVM_UNCERTAIN_BACKOFF_MAX_SECS_OVERRIDE
            .unwrap_or(Self::EVM_UNCERTAIN_BACKOFF_MAX_SECS)
            .max(0)
    }

    fn is_evm_chain_code(chain_code: &str) -> bool {
        chain_code.eq_ignore_ascii_case("eth") || chain_code.eq_ignore_ascii_case("bnb")
    }

    fn should_force_align_prebroadcast_nonce_gap(chain_nonce: u64, local_nonce: u64) -> bool {
        local_nonce > chain_nonce && local_nonce.saturating_sub(chain_nonce) >= 2
    }

    fn evm_uncertain_backoff_secs(retry_count: u32) -> i64 {
        match retry_count {
            0..=3 => 0,
            4..=6 => Self::evm_uncertain_backoff_mid_secs(),
            _ => Self::evm_uncertain_backoff_max_secs(),
        }
    }

    fn evm_uncertain_elapsed_secs(
        req: &ApiWithdrawEntity,
        now: chrono::DateTime<Utc>,
    ) -> Option<i64> {
        req.broadcast_uncertain_since_at
            .map(|since| now.signed_duration_since(since).num_seconds().max(0))
    }

    fn should_mark_evm_uncertain_for_manual_review(
        req: &ApiWithdrawEntity,
        now: chrono::DateTime<Utc>,
    ) -> bool {
        Self::evm_uncertain_elapsed_secs(req, now)
            .map(|elapsed| elapsed >= Self::EVM_UNCERTAIN_MANUAL_REVIEW_TIMEOUT_SECS)
            .unwrap_or(false)
    }

    fn should_throttle_evm_uncertain_recover(
        req: &ApiWithdrawEntity,
        now: chrono::DateTime<Utc>,
    ) -> bool {
        let Some(since) = req.broadcast_uncertain_since_at else {
            return false;
        };
        let elapsed = now.signed_duration_since(since).num_seconds();
        let Some(last_checked) = req.broadcast_uncertain_last_checked_at else {
            return false;
        };
        let wait_secs = if elapsed >= Self::evm_uncertain_timeout_secs() {
            if req.broadcast_uncertain_reconciled_at.is_some() {
                Self::evm_uncertain_backoff_max_secs()
            } else {
                return false;
            }
        } else {
            Self::evm_uncertain_backoff_secs(req.broadcast_uncertain_retry_count)
        };
        if wait_secs <= 0 {
            return false;
        }
        now.signed_duration_since(last_checked).num_seconds() < wait_secs
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

    fn is_tron_missing_confirmed_and_pending_error(err: &ServiceError) -> bool {
        err.to_string().contains("tron tx missing from confirmed and pending pools")
    }

    fn should_rebroadcast_tron_missing_confirmed_and_pending(req: &ApiWithdrawEntity) -> bool {
        if !req.chain_code.eq_ignore_ascii_case("tron") {
            return false;
        }
        let Some(last_broadcast_at) = req.last_broadcast_at else {
            return false;
        };
        Utc::now().signed_duration_since(last_broadcast_at).num_seconds()
            >= Self::TRON_MISSING_CONFIRMED_AND_PENDING_MANUAL_REVIEW_TIMEOUT_SECS
    }

    fn should_invalidate_expired_tron_raw_for_recover(
        chain_code: &str,
        raw_tx_json: &str,
        last_broadcast_at_present: bool,
    ) -> bool {
        !last_broadcast_at_present
            && Self::should_invalidate_expired_tron_raw(chain_code, raw_tx_json)
    }

    pub fn new(
        pool: ApiTransactionDbPool,
        core_pool: ApiWalletDbPool,
        scanner: Arc<ShadowScanner>,
    ) -> Self {
        Self { pool, core_pool, scanner }
    }

    /// 处理命令
    pub async fn handle(&self, command: super::ShadowWithdrawCommand) -> Result<(), ServiceError> {
        match command {
            super::ShadowWithdrawCommand::BuildTx(trade_no) => {
                self.process_build_tx(trade_no).await
            }
            super::ShadowWithdrawCommand::Broadcast(trade_no) => {
                self.process_broadcast(trade_no).await
            }
            super::ShadowWithdrawCommand::Recover(trade_no) => self.process_recover(trade_no).await,
        }
    }

    /// 执行 Recover Command - 外层wrapper，确保所有错误都被捕获
    async fn process_recover(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Processing Recover command");

        // 使用内层函数来捕获所有错误
        if let Err(err) = self.process_recover_inner(&trade_no).await {
            error!(trade_no = %trade_no, error = %err, source = "shadow_withdraw_worker", "Recover inner failed, handling error");
            self.handle_withdraw_tx_failed(&trade_no, WithdrawFailureStage::Chain, err).await?;
        }

        Ok(())
    }

    /// Recover 内部实现，可能返回错误
    async fn process_recover_inner(&self, trade_no: &str) -> Result<(), ServiceError> {
        // ====== phase 1: 并发裁决 ======
        // ⚠️ 禁止任何网络调用、sleep、await RPC
        let req = {
            // 获取提币交易信息
            let initial_req = self.get_withdraw_entity(trade_no).await?;

            // 🔒 必须重新读取，确保基于最新状态做决策
            // ⚠️ 只读"裁决字段"，不做任何业务推断
            let fresh_req = self.get_withdraw_entity(trade_no).await?;

            // 事实校验：Recover 只能处理 tx_hash 不为空且 transaction_time 为空的交易
            // ⚠️ 这里是并发裁决的关键，确保只有一个task能通过
            if fresh_req.tx_hash.is_none()
                || fresh_req.transaction_time.is_some()
                || fresh_req.tx_exec_receipt_uploaded_at.is_some()
            {
                info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "tx_hash empty or transaction_time exists, skipping Recover");
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
                    source = "shadow_withdraw_worker",
                    "Skip Recover: EVM raw_tx exists but not in uncertain state; broadcast should proceed"
                );
                return Ok(());
            }

            fresh_req
        };

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
                    source = "shadow_withdraw_worker",
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
            info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Acquired chain rpc guard permit");
        }

        // 执行恢复交易
        let recover_result = self.recover_tx(&req).await;
        match recover_result {
            Ok(Some(tx_resp)) => {
                info!(trade_no = %trade_no, tx_hash = %tx_resp.tx_hash, source = "shadow_withdraw_worker", "Transaction recover successful");

                // ====== phase 3: 提交不可逆事实 ======
                {
                    // 🔒 必须重新读取，确保基于最新状态做决策
                    let fresh_req = self.get_withdraw_entity(trade_no).await?;

                    // 事实校验：Recover 只能处理 tx_hash 不为空且 transaction_time 为空的交易
                    if fresh_req.tx_hash.is_none() || fresh_req.transaction_time.is_some() {
                        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "tx_hash empty or transaction_time exists, skipping Recover fact commit");
                        return Ok(());
                    }

                    // 🔒 事实保护：检查 tx_hash 一致性，防止事实被覆盖
                    if tx_resp.tx_hash != fresh_req.tx_hash.as_deref().unwrap_or_default() {
                        error!(
                            trade_no = %fresh_req.trade_no,
                            existing_tx_hash = %fresh_req.tx_hash.as_deref().unwrap_or_default(),
                            recover_tx_hash = %tx_resp.tx_hash,
                            source = "shadow_withdraw_worker",
                            "tx_hash mismatch during recover - fact integrity violated"
                        );
                        return Err(ServiceError::System(SystemError::Internal(
                            "recover tx_hash mismatch".to_string(),
                        )));
                    }

                    let resource_consume = if let Some(consumer) = tx_resp.consumer {
                        consumer.energy_used.to_string()
                    } else {
                        "0".to_string()
                    };

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
                    let rows_affected =
                        ApiWithdrawRepo::confirm_onchain_transaction_fact_with_recover(
                            &self.pool,
                            &fresh_req.trade_no,
                            &tx_resp.tx_hash,
                            &transaction_time,
                            &transaction_time,
                            &fresh_req.transaction_fee,
                            &resource_consume,
                        )
                        .await
                        .map_err(|e| ServiceError::Database(e.into()))?;

                    // 显式处理幂等情况：恢复已被其他并发执行
                    if rows_affected == 0 {
                        info!(
                            trade_no = %fresh_req.trade_no,
                            tx_hash = %tx_resp.tx_hash,
                            source = "shadow_withdraw_worker",
                            "update_after_recover skipped: recover already executed (idempotent hit)"
                        );
                    } else {
                        // 直接调用 try_advance 进行点对点唤醒
                        self.scanner.try_advance(&fresh_req.trade_no).await;
                    }
                }
            }
            Ok(None) => {
                info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Transaction recover result is uncertain");
                if let Some(raw_tx_json) = req.raw_tx.as_deref() {
                    if Self::should_invalidate_expired_tron_raw_for_recover(
                        &req.chain_code,
                        raw_tx_json,
                        req.last_broadcast_at.is_some(),
                    ) {
                        warn!(
                            trade_no = %req.trade_no,
                            tx_hash = %req.tx_hash.as_deref().unwrap_or_default(),
                            source = "shadow_withdraw_worker",
                            "Detected expired tron raw_tx during recover; invalidating stale tx facts"
                        );
                        let rows = ApiWithdrawRepo::invalidate_raw_tx(
                            &self.pool,
                            &req.trade_no,
                            None,
                            None,
                            None,
                        )
                        .await
                        .map_err(|e| ServiceError::Database(e.into()))?;
                        if rows > 0 {
                            self.scanner.try_advance(&req.trade_no).await;
                        }
                        return Ok(());
                    }
                }

                if !Self::is_evm_chain_code(&req.chain_code) {
                    // 非 EVM 保持原行为：立即尝试推进一次
                    self.scanner.try_advance(trade_no).await;
                    return Ok(());
                }

                let now = Utc::now();
                let rows_affected =
                    ApiWithdrawRepo::mark_broadcast_uncertain_attempt(&self.pool, trade_no)
                        .await
                        .map_err(|e| ServiceError::Database(e.into()))?;
                let refreshed = self.get_withdraw_entity(trade_no).await?;
                info!(
                    trade_no = %refreshed.trade_no,
                    tx_hash = %refreshed.tx_hash.as_deref().unwrap_or_default(),
                    nonce = refreshed.nonce,
                    rows_affected = %rows_affected,
                    retry_count = refreshed.broadcast_uncertain_retry_count,
                    uncertain_since_at = ?refreshed.broadcast_uncertain_since_at,
                    reconciled_at = ?refreshed.broadcast_uncertain_reconciled_at,
                    rebroadcast_count = refreshed.broadcast_uncertain_rebroadcast_count,
                    source = "shadow_withdraw_worker",
                    "EVM recover uncertain state recorded"
                );

                let elapsed_secs =
                    Self::evm_uncertain_elapsed_secs(&refreshed, now).unwrap_or_default();
                let timed_out = elapsed_secs >= Self::evm_uncertain_timeout_secs();
                if !timed_out {
                    return Ok(());
                }

                if refreshed.broadcast_uncertain_reconciled_at.is_none() {
                    warn!(
                        trade_no = %refreshed.trade_no,
                        tx_hash = %refreshed.tx_hash.as_deref().unwrap_or_default(),
                        nonce = refreshed.nonce,
                        uncertain_duration_sec = elapsed_secs,
                        source = "shadow_withdraw_worker",
                        "EVM uncertain timeout reached; running nonce reconcile"
                    );

                    let nonce_engine = get_nonce_engine();
                    nonce_engine.trigger_reconcile_with_reason(
                        &refreshed.from_addr,
                        &refreshed.chain_code,
                        ReconcileReason::Other("evm_uncertain_timeout".to_string()),
                        true,
                    );
                    let _ = ApiWithdrawRepo::mark_broadcast_uncertain_reconciled(
                        &self.pool,
                        &refreshed.trade_no,
                    )
                    .await
                    .map_err(|e| ServiceError::Database(e.into()))?;
                }

                if !Self::should_mark_evm_uncertain_for_manual_review(&refreshed, now) {
                    warn!(
                        trade_no = %refreshed.trade_no,
                        tx_hash = %refreshed.tx_hash.as_deref().unwrap_or_default(),
                        nonce = refreshed.nonce,
                        uncertain_duration_sec = elapsed_secs,
                        reconcile_done = %refreshed.broadcast_uncertain_reconciled_at.is_some(),
                        source = "shadow_withdraw_worker",
                        "EVM uncertain timeout reached; keep frozen and continue observing"
                    );
                    return Ok(());
                }

                warn!(
                    trade_no = %refreshed.trade_no,
                    tx_hash = %refreshed.tx_hash.as_deref().unwrap_or_default(),
                    nonce = refreshed.nonce,
                    uncertain_duration_sec = elapsed_secs,
                    reconcile_done = %refreshed.broadcast_uncertain_reconciled_at.is_some(),
                    rebroadcast_count = refreshed.broadcast_uncertain_rebroadcast_count,
                    source = "shadow_withdraw_worker",
                    "EVM uncertain exceeded manual review timeout; marking failed for human handling"
                );

                let error_msg = format!(
                    "EVM broadcast uncertain exceeded manual review timeout after {}s; human intervention required",
                    Self::EVM_UNCERTAIN_MANUAL_REVIEW_TIMEOUT_SECS
                );
                let rows_affected = ApiWithdrawRepo::update_api_withdraw_status_and_err(
                    &self.pool,
                    &refreshed.trade_no,
                    wallet_database::entities::api_withdraw::ApiWithdrawStatus::SendingTxFailed,
                    ErrCode::TransactionOnChainException,
                    &error_msg,
                )
                .await
                .map_err(|db_err: wallet_database::Error| {
                    error!(trade_no = %refreshed.trade_no, error = %db_err, source = "shadow_withdraw_worker", "Failed to mark EVM uncertain manual-review timeout as failed");
                    ServiceError::Database(db_err.into())
                })?;

                let stage_rows = ApiWithdrawRepo::set_failure_stage(
                    &self.pool,
                    &refreshed.trade_no,
                    WithdrawFailureStage::Chain,
                )
                .await
                .map_err(|db_err: wallet_database::Error| {
                    error!(trade_no = %refreshed.trade_no, error = %db_err, source = "shadow_withdraw_worker", "Failed to set withdraw failure_stage for manual-review timeout");
                    ServiceError::Database(db_err.into())
                })?;

                if rows_affected > 0 || stage_rows > 0 {
                    self.scanner.try_advance(&refreshed.trade_no).await;
                }
                return Ok(());
            }
            Err(err) if Self::is_tron_missing_confirmed_and_pending_error(&err) => {
                if !Self::should_rebroadcast_tron_missing_confirmed_and_pending(&req) {
                    info!(
                        trade_no = %trade_no,
                        tx_hash = %req.tx_hash.as_deref().unwrap_or_default(),
                        source = "shadow_withdraw_worker",
                        "Tron tx missing from confirmed and pending pools; keep observing before manual review"
                    );
                    return Ok(());
                }

                warn!(
                    trade_no = %trade_no,
                    tx_hash = %req.tx_hash.as_deref().unwrap_or_default(),
                    source = "shadow_withdraw_worker",
                    "Tron tx missing from confirmed and pending pools beyond timeout; marking manual handling"
                );
                let rows_affected = ApiWithdrawRepo::update_api_withdraw_status_and_err(
                    &self.pool,
                    &req.trade_no,
                    wallet_database::entities::api_withdraw::ApiWithdrawStatus::SendingTxFailed,
                    ErrCode::TransactionOnChainException,
                    "Tron broadcast missing from both confirmed and pending pools after timeout; manual intervention required",
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
                let stage_rows = ApiWithdrawRepo::set_failure_stage(
                    &self.pool,
                    &req.trade_no,
                    WithdrawFailureStage::Chain,
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
                if rows_affected > 0 || stage_rows > 0 {
                    self.scanner.try_advance(&req.trade_no).await;
                }
            }
            Err(err) => return Err(err),
        }

        Ok(())
    }

    /// 执行 BuildTx Command - 外层wrapper，确保所有错误都被捕获
    async fn process_build_tx(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Processing BuildTx command");

        // 使用内层函数来捕获所有错误
        if let Err(err) = self.process_build_tx_inner(&trade_no).await {
            error!(trade_no = %trade_no, error = %err, source = "shadow_withdraw_worker", "BuildTx inner failed, handling error");
            self.handle_withdraw_tx_failed(&trade_no, WithdrawFailureStage::Build, err).await?;
        }

        Ok(())
    }

    /// BuildTx 内部实现，可能返回错误
    async fn process_build_tx_inner(&self, trade_no: &str) -> Result<(), ServiceError> {
        // 1. 从数据库中获取提币交易信息
        let withdraw = self.get_withdraw_entity(trade_no).await?;

        // check
        if !self.check_digest(&withdraw).await? {
            tracing::error!(trade_no=%trade_no, "[提币] 交易数据验证失败");
            return Err(ServiceError::Business(
                ApiWalletError::Trans(TransError::TransactionDigestVerificationFailed).into(),
            ));
        }
        tracing::info!(trade_no=%trade_no, "[提币] 交易数据验证通过");

        // ====== phase 1: 快速检查 ======
        // ⚠️ 禁止任何网络调用、sleep、await RPC
        let nonce = {
            // 🔒 必须重新读取，确保基于最新状态做决策
            // ⚠️ 只读"裁决字段"，不做任何业务推断
            let fresh_withdraw = self.get_withdraw_entity(trade_no).await?;

            // 2. 事实校验：BuildTx 只能处理 raw_tx 为空的交易
            // ⚠️ 这里是并发裁决的关键，确保只有一个task能通过
            if fresh_withdraw.raw_tx.is_some() {
                info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "raw_tx already exists, skipping BuildTx");
                return Ok(());
            }

            // 7. 获取并更新 nonce
            let nonce =
                self.get_nonce(&fresh_withdraw.from_addr, &fresh_withdraw.chain_code).await?;
            info!(trade_no = %trade_no, nonce = %nonce, source = "shadow_withdraw_worker", "Retrieved nonce");

            nonce
        };

        // 先占位 build slot，防止同一 trade_no 在构建期间被重复推进。
        let build_slot_rows = ApiWithdrawRepo::update_building_at(&self.pool, trade_no).await?;
        if build_slot_rows == 0 {
            info!(
                trade_no = %trade_no,
                source = "shadow_withdraw_worker",
                "Build slot already claimed or recently updated, skipping BuildTx"
            );
            return Ok(());
        }

        // ====== phase 2: 网络执行 ======
        // 获取链交互全局许可（按 guarded endpoint 控制并发）
        let _chain_rpc_guard =
            crate::infrastructure::chain_rpc_guard::acquire_if_guarded(&withdraw.chain_code).await;
        if _chain_rpc_guard.is_some() {
            info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Acquired chain rpc guard permit");
        }

        // 通过Context获取Handles实例，然后获取私钥管理器
        let handles = crate::context::get_context()?.get_handles_arc().await?;
        let private_key_manager = handles.get_global_private_key_manager();
        let private_key =
            private_key_manager.get_private_key(&withdraw.from_addr, &withdraw.chain_code).await?;
        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Retrieved private key from manager");

        // 9. 生成转账请求
        // ⚠️ nonce 只在 phase 1 分配，这里直接传入
        let transfer_req = self.gen_transfer_req(&withdraw, nonce).await?;
        info!(trade_no = %trade_no, nonce = %nonce, source = "shadow_withdraw_worker", "Generated transfer request with nonce");

        // 10. 构建交易
        let (tx_hash, raw_tx, fee_str) = ApiTransDomain::build_transfer_raw(
            transfer_req,
            Some(private_key), // 私钥管理
        )
        .await?;
        info!(trade_no = %trade_no, tx_hash = %tx_hash, fee = %fee_str, source = "shadow_withdraw_worker", "Built transfer raw transaction successfully");

        // ====== phase 3: 提交不可逆事实 ======
        {
            // 11. 立即将tx_hash、raw_tx和nonce存储到数据库
            let raw_tx_str = wallet_utils::serde_func::serde_to_string(&raw_tx)?;
            let rows_affected = ApiWithdrawRepo::update_after_build(
                &self.pool,
                &withdraw.trade_no,
                &tx_hash,
                &raw_tx_str,
                &fee_str,
                nonce as i64,
            )
            .await?;

            // 显式处理幂等情况：如果影响行数为0，表示raw_tx已存在或被并发写入
            if rows_affected == 0 {
                info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "update_after_build skipped: raw_tx already exists (idempotent hit)");
                return Ok(());
            }

            info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Updated tx_hash, raw_tx and nonce to database successfully");

            // 直接调用 try_advance 进行点对点唤醒
            self.scanner.try_advance(&withdraw.trade_no).await;
        }

        Ok(())
    }

    /// 执行 Broadcast Command - 外层wrapper，确保所有错误都被捕获
    async fn process_broadcast(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Processing Broadcast command");

        // 使用内层函数来捕获所有错误
        if let Err(err) = self.process_broadcast_inner(&trade_no).await {
            error!(trade_no = %trade_no, error = %err, source = "shadow_withdraw_worker", "Broadcast inner failed, handling error");
            self.handle_withdraw_tx_failed(&trade_no, WithdrawFailureStage::Broadcast, err).await?;
        }

        Ok(())
    }

    /// Broadcast 内部实现，可能返回错误
    async fn process_broadcast_inner(&self, trade_no: &str) -> Result<(), ServiceError> {
        // 1. 从数据库中获取提币交易信息
        let withdraw = self.get_withdraw_entity(trade_no).await?;

        // ====== phase 1: 快速检查 ======
        // ⚠️ 禁止任何网络调用、sleep、await RPC
        {
            // 🔒 必须重新读取，确保基于最新状态做决策
            // ⚠️ 只读"裁决字段"，不做任何业务推断
            let fresh_withdraw = self.get_withdraw_entity(trade_no).await?;

            // 2. 事实校验：Broadcast 只能处理 raw_tx 存在的交易
            // 🔒 与 predicate::can_broadcast 同构，确保模型自洽
            // ⚠️ 这里是并发裁决的关键，确保只有一个task能通过
            if fresh_withdraw.raw_tx.is_none() {
                info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "raw_tx empty, skipping Broadcast");
                return Ok(());
            }

            if Self::is_evm_chain_code(&fresh_withdraw.chain_code)
                && fresh_withdraw.broadcast_uncertain_since_at.is_some()
                && fresh_withdraw.last_broadcast_at.is_none()
                && fresh_withdraw.transaction_time.is_none()
                && fresh_withdraw.tx_exec_receipt_uploaded_at.is_none()
                && fresh_withdraw.err_code.is_none()
            {
                info!(
                    trade_no = %trade_no,
                    tx_hash = %fresh_withdraw.tx_hash.as_deref().unwrap_or_default(),
                    nonce = fresh_withdraw.nonce,
                    source = "shadow_withdraw_worker",
                    "Skip Broadcast: EVM uncertain state in progress; recover should proceed"
                );
                return Ok(());
            }

            if Self::is_evm_chain_code(&fresh_withdraw.chain_code) {
                match ApiTransDomain::nonce(&fresh_withdraw.from_addr, &fresh_withdraw.chain_code)
                    .await
                {
                    Ok(chain_nonce) => {
                        if Self::should_force_align_prebroadcast_nonce_gap(
                            chain_nonce,
                            fresh_withdraw.nonce as u64,
                        ) {
                            let gap = fresh_withdraw.nonce as u64 - chain_nonce;
                            warn!(
                                trade_no = %fresh_withdraw.trade_no,
                                from_addr = %fresh_withdraw.from_addr,
                                tx_hash = %fresh_withdraw.tx_hash.as_deref().unwrap_or_default(),
                                chain_code = %fresh_withdraw.chain_code,
                                chain_nonce = %chain_nonce,
                                local_nonce = %fresh_withdraw.nonce,
                                gap = %gap,
                                source = "shadow_withdraw_worker",
                                "EVM pre-broadcast nonce-gap detected; best-effort force align"
                            );

                            let nonce_engine = get_nonce_engine();
                            if let Err(e) = nonce_engine
                                .force_align_to_chain_next_nonce(
                                    &fresh_withdraw.from_addr,
                                    &fresh_withdraw.chain_code,
                                    ReconcileReason::Other(
                                        "evm_prebroadcast_nonce_gap".to_string(),
                                    ),
                                )
                                .await
                            {
                                warn!(
                                    trade_no = %fresh_withdraw.trade_no,
                                    error = %e,
                                    source = "shadow_withdraw_worker",
                                    "Best-effort nonce force-align failed; continue broadcast"
                                );
                            }
                            info!(
                                trade_no = %fresh_withdraw.trade_no,
                                source = "shadow_withdraw_worker",
                                "Best-effort pre-broadcast nonce check completed; continue broadcast"
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            trade_no = %fresh_withdraw.trade_no,
                            error = %e,
                            source = "shadow_withdraw_worker",
                            "Best-effort pre-broadcast nonce check failed; continue broadcast"
                        );
                    }
                }
            }

            // 3. 检查是否已有raw_tx和tx_hash
            if fresh_withdraw.tx_hash.is_none() || fresh_withdraw.raw_tx.is_none() {
                error!(trade_no = %trade_no, source = "shadow_withdraw_worker", "No raw_tx or tx_hash found");
                return Err(ServiceError::Business(
                    ApiWalletError::Trans(crate::error::business::api_wallet::trans::TransError::BuildWithdrawTransactionFailed("Missing transaction data".to_string())).into(),
                ));
            }
        }

        // ====== phase 2: 网络执行 ======
        // 获取链交互全局许可（按 guarded endpoint 控制并发）
        let _chain_rpc_guard =
            crate::infrastructure::chain_rpc_guard::acquire_if_guarded(&withdraw.chain_code).await;
        if _chain_rpc_guard.is_some() {
            info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Acquired chain rpc guard permit");
        }

        // 6. 反序列化raw_tx
        let raw_tx = wallet_utils::serde_func::serde_from_str(
            &withdraw.raw_tx.as_deref().unwrap_or_default(),
        )?;
        info!(trade_no = %trade_no, tx_hash = %withdraw.tx_hash.as_deref().unwrap_or_default(), source = "shadow_withdraw_worker", "Deserialized raw_tx successfully");

        // 7. 广播交易
        info!(trade_no = %trade_no, tx_hash = %withdraw.tx_hash.as_deref().unwrap_or_default(), source = "shadow_withdraw_worker", "Starting to broadcast transaction");
        let tx_resp = ApiTransDomain::broadcast_transfer(
            &withdraw.chain_code,
            raw_tx,
            withdraw.tx_hash.as_deref(),
        )
        .await?;

        match tx_resp {
            Some(tx) => {
                info!(trade_no = %trade_no, tx_hash = %tx.tx_hash, source = "shadow_withdraw_worker", "Transaction broadcast successful");

                // 🔒 事实保护：检查 tx_hash 一致性，防止 build 阶段事实被覆盖
                if let Some(existing) = withdraw.tx_hash.as_deref() {
                    if existing != tx.tx_hash {
                        error!(
                            trade_no = %withdraw.trade_no,
                            existing_tx_hash = %existing,
                            broadcast_tx_hash = %tx.tx_hash,
                            source = "shadow_withdraw_worker",
                            "tx_hash mismatch between build and broadcast - fact integrity violated"
                        );
                        return Err(ServiceError::System(SystemError::Internal(
                            "Invariant broken - tx_hash mismatch between build and broadcast"
                                .to_string(),
                        )));
                    }
                }

                // ====== phase 3: 提交不可逆事实 ======
                {
                    // 广播成功 = 一次不可分割的事实提交
                    let resource_consume = if let Some(consumer) = tx.consumer {
                        consumer.energy_used.to_string()
                    } else {
                        "0".to_string()
                    };

                    let rows_affected =
                        ApiWithdrawRepo::mark_broadcast_executed(&self.pool, &withdraw.trade_no)
                            .await
                            .map_err(|e| ServiceError::Database(e.into()))?;

                    // 显式处理幂等情况：广播已被其他并发/恢复执行
                    if rows_affected == 0 {
                        info!(
                            trade_no = %withdraw.trade_no,
                            tx_hash = %tx.tx_hash,
                            source = "shadow_withdraw_worker",
                            "mark_broadcast_executed skipped: broadcast already executed (idempotent hit)"
                        );
                    } else {
                        // 直接调用 try_advance 进行点对点唤醒
                        self.scanner.try_advance(&withdraw.trade_no).await;
                    }
                }

                Ok(())
            }
            None => {
                info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Transaction broadcast result is uncertain");
                if !Self::is_evm_chain_code(&withdraw.chain_code) {
                    return Ok(());
                }
                let had_uncertain_since = withdraw.broadcast_uncertain_since_at.is_some();
                let rows_affected =
                    ApiWithdrawRepo::mark_broadcast_uncertain_attempt(&self.pool, trade_no)
                        .await
                        .map_err(|e| ServiceError::Database(e.into()))?;
                let refreshed = self.get_withdraw_entity(trade_no).await?;
                info!(
                    trade_no = %refreshed.trade_no,
                    tx_hash = %refreshed.tx_hash.as_deref().unwrap_or_default(),
                    nonce = refreshed.nonce,
                    rows_affected = %rows_affected,
                    had_uncertain_since = had_uncertain_since,
                    retry_count = refreshed.broadcast_uncertain_retry_count,
                    uncertain_since_at = ?refreshed.broadcast_uncertain_since_at,
                    source = "shadow_withdraw_worker",
                    "EVM broadcast uncertain state recorded"
                );
                self.scanner.try_advance(trade_no).await;
                Ok(())
            }
        }
    }

    /// 从数据库中获取提币交易信息
    async fn get_withdraw_entity(
        &self,
        trade_no: &str,
    ) -> Result<wallet_database::entities::api_withdraw::ApiWithdrawEntity, ServiceError> {
        let entity = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &self.pool,
            trade_no,
            wallet_database::entities::api_trade_type::ApiTradeType::Withdraw,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        Ok(entity)
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
        info!(from_addr = %from_addr, chain_code = %chain_code, source = "shadow_withdraw_worker", "Getting nonce");
        let chain: ChainCode = chain_code.try_into()?;

        // ⚠️ EVM nonce MUST be allocated via DB atomic upsert.
        // Any read-modify-write logic here is forbidden.
        match chain {
            c if Self::is_evm_chain(&c) => {
                // 对于以太坊类链，使用NonceEngine来分配nonce
                // ⚠️ INVARIANT:
                // This method MUST guarantee DB-level atomic CAS for nonce allocation.
                // Any refactor breaking this invariant will cause nonce duplication.
                use crate::infrastructure::nonce::nonce_engine::get_nonce_engine;

                let nonce_engine = get_nonce_engine();
                let nonce = nonce_engine.allocate_nonce(from_addr, chain_code, &self.pool).await?;
                info!(from_addr = %from_addr, chain_code = %chain_code, nonce = %nonce, source = "shadow_withdraw_worker", "Retrieved nonce using NonceEngine");
                Ok(nonce as u64)
            }
            _ => {
                // 非 EVM 链不参与 nonce 分配
                Ok(0)
            }
        }
    }

    async fn check_digest(&self, req: &ApiWithdrawEntity) -> Result<bool, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "[提币] 验证交易摘要");
        let sn = crate::context::get_context().unwrap().get_sn();
        let mut d = wallet_utils::conversion::decimal_from_str(req.value.as_str())?;
        d = d.normalize();
        let raw_data = req.from_addr.clone() + req.to_addr.as_str() + d.to_string().as_str() + sn;
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(&raw_data));
        let is_valid = req.validate == digest;
        tracing::info!(trade_no=%req.trade_no, "[提币] 摘要验证结果: {}", is_valid);
        Ok(is_valid)
    }

    /// 生成转账请求
    ///
    /// 🔒 关键语义：
    /// - nonce 由调用方传入，不再内部获取
    /// - nonce 已经在 phase 1 中分配，这里只使用
    /// - 确保 nonce 从"动态信息"升级为"已裁决事实"
    async fn gen_transfer_req(
        &self,
        req: &ApiWithdrawEntity,
        nonce: u64,
    ) -> Result<ApiTransferReq, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, from_addr=%req.from_addr, to_addr=%req.to_addr, value=%req.value, "[提币] 创建基础转账请求");

        // 获取币种信息
        let coin = ApiCoinDomain::get_coin_by_token_key_exact(
            &req.chain_code,
            req.token_addr.clone().into(),
        )
        .await?;
        tracing::info!(trade_no=%req.trade_no, "提币:send: 获取币种信息成功, symbol={}, token_address={:?}, decimals={}", 
            coin.symbol, coin.token_address, coin.decimals);

        let mut params =
            ApiBaseTransferReq::new(&req.from_addr, &req.to_addr, &req.value, &req.chain_code);

        let token_address = if coin.token_address.is_native() {
            None
        } else {
            let s = coin.token_address.as_db_str().to_string();
            if s.is_empty() { None } else { Some(s) }
        };
        params.with_token(token_address, coin.decimals, &coin.symbol);
        tracing::info!(trade_no=%req.trade_no, "提币:send: 创建基础转账请求成功");

        tracing::info!(trade_no=%req.trade_no, "[提币] 获取钱包解锁态");
        let unlock_token = ApiWalletDomain::get_wallet_unlock_token().await?;

        let transfer_req = ApiTransferReq { base: params, password: unlock_token, nonce };
        tracing::info!(trade_no=%req.trade_no, nonce=%nonce, "[提币] 转账请求生成完成");
        Ok(transfer_req)
    }

    /// 交易恢复逻辑
    ///
    /// ⚠️ IMPORTANT:
    /// - Recover logic MUST only be triggered by Scanner commands
    /// - This method should NOT be called directly by other components
    /// - On-chain confirmation fact is owned by Scanner / Shadow Recovery ONLY
    async fn recover_tx(
        &self,
        withdraw: &wallet_database::entities::api_withdraw::ApiWithdrawEntity,
    ) -> Result<Option<crate::domain::chain::TransferResp>, ServiceError> {
        let tx_hash = withdraw.tx_hash.as_ref().unwrap();
        info!(trade_no = %withdraw.trade_no, tx_hash = %tx_hash, source = "shadow_withdraw_worker", "Processing recovered tx");

        if Self::is_evm_chain_code(&withdraw.chain_code) {
            info!(
                trade_no = %withdraw.trade_no,
                tx_hash = %tx_hash,
                chain_code = %withdraw.chain_code,
                force_recover_none = Self::test_force_withdraw_evm_recover_none(),
                uncertain_timeout_secs = Self::evm_uncertain_timeout_secs(),
                uncertain_backoff_mid_secs = Self::evm_uncertain_backoff_mid_secs(),
                uncertain_backoff_max_secs = Self::evm_uncertain_backoff_max_secs(),
                source = "shadow_withdraw_worker",
                "withdraw EVM recover test switches snapshot"
            );
        }

        if Self::is_evm_chain_code(&withdraw.chain_code)
            && Self::test_force_withdraw_evm_recover_none()
        {
            warn!(
                trade_no = %withdraw.trade_no,
                tx_hash = %tx_hash,
                chain_code = %withdraw.chain_code,
                source = "shadow_withdraw_worker",
                "TEST STUB: force withdraw EVM recover result None"
            );
            return Ok(None);
        }

        match ApiTransDomain::process_recovered_tx(
            &withdraw.chain_code,
            &withdraw.from_addr,
            tx_hash,
            withdraw.nonce,
            &withdraw.transaction_fee,
        )
        .await
        {
            Ok(Some(tx_resp)) => {
                info!(trade_no = %withdraw.trade_no, tx_hash = %tx_hash, source = "shadow_withdraw_worker", "Recovered tx success");
                Ok(Some(tx_resp))
            }
            Ok(None) => {
                info!(trade_no = %withdraw.trade_no, tx_hash = %tx_hash, source = "shadow_withdraw_worker", "Recovered tx result is uncertain, will retry");
                Ok(None)
            }
            Err(err) => {
                error!(trade_no = %withdraw.trade_no, tx_hash = %tx_hash, error = %err, source = "shadow_withdraw_worker", "Recovered tx failed");
                Err(err)
            }
        }
    }

    /// 处理提币交易失败
    async fn handle_withdraw_tx_failed(
        &self,
        trade_no: &str,
        stage: WithdrawFailureStage,
        err: ServiceError,
    ) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, error = %err, source = "shadow_withdraw_worker", "Handling withdraw tx failed");

        // 🔒 事实保护：检查是否已存在成功事实
        // 规则：一旦成功事实（transaction_time）成立，失败事实永远不能覆盖它
        // 这是事实系统的"单调性约束"
        let withdraw = self.get_withdraw_entity(trade_no).await?;
        if withdraw.transaction_time.is_some() {
            info!(
                trade_no = %trade_no,
                source = "shadow_withdraw_worker",
                "Skip mark failed: transaction already confirmed (monotonicity constraint)"
            );
            return Ok(());
        }

        // BuildTx 失败只需要释放 build slot，让 scanner 后续重试。
        // 这里不写失败事实，避免把可重试的构建失败误记成终态失败。
        if matches!(stage, WithdrawFailureStage::Build) && withdraw.raw_tx.is_none() {
            let rows_affected = ApiWithdrawRepo::clear_building_at(&self.pool, trade_no)
                .await
                .map_err(|db_err: wallet_database::Error| {
                    error!(trade_no = %trade_no, error = %db_err, source = "shadow_withdraw_worker", "Failed to clear build slot for BuildTx failure");
                    ServiceError::Database(db_err.into())
                })?;

            info!(
                trade_no = %trade_no,
                rows_affected = %rows_affected,
                source = "shadow_withdraw_worker",
                "BuildTx failure cleared build slot"
            );

            return Ok(());
        }

        // Solana 广播常见可恢复错误：
        // - blockhash 过期（Blockhash not found）
        // 这类错误不应直接写失败事实，而应作废 raw_tx/tx_hash 触发重建。
        if matches!(stage, WithdrawFailureStage::Broadcast)
            && withdraw.chain_code.eq_ignore_ascii_case("sol")
            && ApiTransDomain::is_blockhash_not_found_error(&err)
            && withdraw.raw_tx.is_some()
            && withdraw.tx_hash.is_some()
            && withdraw.last_broadcast_at.is_none()
        {
            let rows_affected =
                ApiWithdrawRepo::invalidate_raw_tx(&self.pool, trade_no, None, None, None)
                    .await
                    .map_err(|db_err: wallet_database::Error| {
                        error!(trade_no = %trade_no, error = %db_err, source = "shadow_withdraw_worker", "Failed to invalidate raw_tx for sol blockhash rebuild");
                        ServiceError::Database(db_err.into())
                    })?;
            info!(
                trade_no = %trade_no,
                rows_affected = %rows_affected,
                error = %err,
                source = "shadow_withdraw_worker",
                "SOL blockhash not found detected, invalidated raw_tx for rebuild"
            );
            if rows_affected > 0 {
                self.scanner.try_advance(trade_no).await;
            }
            return Ok(());
        }

        // 广播阶段的 "already exists" 表示链节点已接受该交易，属于幂等成功。
        // 兜底处理：即使上游未命中 duplicate 判定，也不应写 SendingTxFailed。
        if matches!(stage, WithdrawFailureStage::Broadcast)
            && ApiTransDomain::is_duplicate_broadcast_error(&err)
        {
            let rows_affected = ApiWithdrawRepo::mark_broadcast_executed(&self.pool, trade_no)
                .await
                .map_err(|db_err: wallet_database::Error| {
                    error!(trade_no = %trade_no, error = %db_err, source = "shadow_withdraw_worker", "Failed to mark broadcast executed for duplicate broadcast");
                    ServiceError::Database(db_err.into())
                })?;
            info!(
                trade_no = %trade_no,
                rows_affected = %rows_affected,
                error = %err,
                source = "shadow_withdraw_worker",
                "Duplicate broadcast detected, treat as idempotent success and continue"
            );
            self.scanner.try_advance(trade_no).await;
            return Ok(());
        }

        let error_msg = format!("{}", err);
        if matches!(stage, WithdrawFailureStage::Broadcast)
            && ApiTransDomain::should_treat_nonce_too_low_as_broadcast_success(
                &withdraw.chain_code,
                &err,
                withdraw.raw_tx.is_some(),
                withdraw.tx_hash.is_some(),
            )
        {
            info!(
                trade_no = %trade_no,
                source = "shadow_withdraw_worker",
                "Detected EVM nonce too low on broadcast, treat as idempotent success"
            );

            let nonce_engine = crate::infrastructure::nonce::nonce_engine::get_nonce_engine();
            if let Err(e) = nonce_engine
                .handle_nonce_error(&withdraw.from_addr, &withdraw.chain_code, &error_msg)
                .await
            {
                warn!(trade_no = %trade_no, error = %e, source = "shadow_withdraw_worker", "Nonce self-heal failed");
            }

            let rows_affected = ApiWithdrawRepo::mark_broadcast_executed(&self.pool, trade_no)
                .await
                .map_err(|db_err: wallet_database::Error| {
                    error!(trade_no = %trade_no, error = %db_err, source = "shadow_withdraw_worker", "Failed to mark broadcast executed for nonce too low broadcast conflict");
                    ServiceError::Database(db_err.into())
                })?;
            info!(
                trade_no = %trade_no,
                rows_affected = %rows_affected,
                error = %err,
                source = "shadow_withdraw_worker",
                "Nonce too low broadcast conflict treated as idempotent success"
            );
            self.scanner.try_advance(trade_no).await;
            return Ok(());
        }

        // 处理 nonce too low 错误
        if ApiTransDomain::is_nonce_too_low_error(&err) {
            info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Detected nonce too low error, syncing nonce from chain");

            let nonce_engine = crate::infrastructure::nonce::nonce_engine::get_nonce_engine();
            if let Err(e) = nonce_engine
                .handle_nonce_error(&withdraw.from_addr, &withdraw.chain_code, &error_msg)
                .await
            {
                warn!(trade_no = %trade_no, error = %e, source = "shadow_withdraw_worker", "Nonce self-heal failed");
            }

            let rows_affected = ApiWithdrawRepo::update_api_withdraw_status_and_err(
                &self.pool,
                trade_no,
                wallet_database::entities::api_withdraw::ApiWithdrawStatus::SendingTxFailed,
                ErrCode::SDKInternalError, // 使用通用错误码
                &error_msg,
            )
            .await
            .map_err(|db_err: wallet_database::Error| {
                error!(trade_no = %trade_no, error = %db_err, source = "shadow_withdraw_worker", "Failed to update status to failed");
                ServiceError::Database(db_err.into())
            })?;

            info!(trade_no = %trade_no, rows_affected = %rows_affected, source = "shadow_withdraw_worker", "Updated status to failed with nonce error");

            // 只有第一次写入失败事实才发送 Tick
            if rows_affected > 0 {
                self.scanner.try_advance(trade_no).await;
            }

            return Ok(());
        }

        // 根据错误类型确定错误码
        match err.retry_policy() {
            wallet_utils::error::RetryPolicy::Never => {
                let err_code = if err.is_network_error() {
                    ErrCode::NetworkException
                } else {
                    ErrCode::SDKInternalError
                };

                let rows_affected = ApiWithdrawRepo::update_api_withdraw_status_and_err(
                    &self.pool,
                    trade_no,
                    wallet_database::entities::api_withdraw::ApiWithdrawStatus::SendingTxFailed,
                    err_code,
                    &error_msg,
                )
                .await
                .map_err(|db_err: wallet_database::Error| {
                    error!(trade_no = %trade_no, error = %db_err, source = "shadow_withdraw_worker", "Failed to update status to failed");
                    ServiceError::Database(db_err.into())
                })?;
                info!(trade_no = %trade_no, rows_affected = %rows_affected, source = "shadow_withdraw_worker", "Updated status to failed");

                // 写入失败阶段事实（幂等）
                // 目的：
                // - 事实驱动的状态推导可在 report_trigger 后进入 SendingTxFailedReport
                // - Diagnose 日志能明确失败发生在哪个阶段
                let stage_rows =
                    match ApiWithdrawRepo::set_failure_stage(&self.pool, trade_no, stage).await {
                        Ok(r) => r,
                        Err(e) => {
                            error!(
                                trade_no = %trade_no,
                                failure_stage = ?stage,
                                error = %e,
                                source = "shadow_withdraw_worker",
                                "Failed to set withdraw failure_stage"
                            );
                            0
                        }
                    };

                // 只有第一次写入失败事实才发送 Tick
                if rows_affected > 0 || stage_rows > 0 {
                    // 直接调用 try_advance 进行点对点唤醒
                    self.scanner.try_advance(trade_no).await;
                }

                // 注意：Shadow Worker 是执行者，不是裁决者
                // 不设置 finished_at，因为链上事实尚未闭环
                // 只有 Scanner/Shadow Recovery 才能设置终态
            }
            wallet_utils::error::RetryPolicy::Delay => {
                tracing::info!(trade_no = %trade_no, error = %err, source = "shadow_withdraw_worker", "Withdraw tx failed, will retry later");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ShadowWithdrawWorker;
    use crate::domain::api_wallet::adapter::tx::RawTx;
    use chrono::Utc;
    use wallet_chain_interact::{BillResourceConsume, tron::operations::RawTransactionParams};
    use wallet_database::entities::{
        api_trade_type::ApiTradeType,
        api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
        asset_token_key::AssetTokenKey,
    };

    fn base_withdraw() -> ApiWithdrawEntity {
        let now = Utc::now();
        ApiWithdrawEntity {
            id: 0,
            name: "withdraw".to_string(),
            uid: "uid".to_string(),
            from_addr: "from".to_string(),
            to_addr: "to".to_string(),
            value: "1".to_string(),
            validate: "digest".to_string(),
            chain_code: "eth".to_string(),
            token_addr: AssetTokenKey::Native,
            symbol: "USDT".to_string(),
            trade_no: "W_EVM_UNCERTAIN_TEST".to_string(),
            trade_type: ApiTradeType::Withdraw,
            init_status: ApiWithdrawStatus::SendingTx,
            status: ApiWithdrawStatus::SendingTx,
            nonce: 3,
            tx_hash: Some("0xhash".to_string()),
            raw_tx: Some("{}".to_string()),
            resource_consume: "0".to_string(),
            transaction_fee: "0".to_string(),
            transaction_time: None,
            block_height: None,
            notes: None,
            post_tx_count: 0,
            post_confirm_tx_count: 0,
            err_code: None,
            err_msg: None,
            tx_ack_sent_at: Some(now),
            building_at: None,
            last_broadcast_at: Some(now),
            broadcast_uncertain_since_at: Some(now - chrono::Duration::minutes(10)),
            broadcast_uncertain_retry_count: 5,
            broadcast_uncertain_last_checked_at: Some(now),
            broadcast_uncertain_reconciled_at: None,
            broadcast_uncertain_rebroadcast_count: 0,
            tx_res_ack_sent_at: None,
            tx_res_received_at: None,
            tx_exec_receipt_uploaded_at: None,
            finished_at: None,
            audit_passed_at: Some(now),
            audit_rejected_at: None,
            audit_reason: None,
            chain_success_at: None,
            chain_failed_at: None,
            failure_stage: None,
            created_at: now,
            updated_at: None,
        }
    }

    fn expired_tron_raw_tx_json(expiration_ms: i64) -> String {
        let raw = RawTransactionParams {
            tx_id: "expired-tron-withdraw-tx".to_string(),
            raw_data: serde_json::json!({ "expiration": expiration_ms }).to_string(),
            raw_data_hex: "00".to_string(),
            signature: vec![],
        };
        let raw_tx =
            RawTx::Tron(raw, BillResourceConsume { net_used: 0, energy_used: 0 }, String::new());
        wallet_utils::serde_func::serde_to_string(&raw_tx).expect("serialize expired tron raw tx")
    }

    #[test]
    fn withdraw_evm_uncertain_timeout_does_not_rebroadcast() {
        let mut withdraw = base_withdraw();
        withdraw.broadcast_uncertain_reconciled_at = Some(Utc::now());

        assert!(ShadowWithdrawWorker::should_throttle_evm_uncertain_recover(&withdraw, Utc::now()));
    }

    #[test]
    fn withdraw_evm_uncertain_before_timeout_still_uses_backoff() {
        let mut withdraw = base_withdraw();
        withdraw.broadcast_uncertain_since_at = Some(Utc::now() - chrono::Duration::minutes(1));
        withdraw.broadcast_uncertain_reconciled_at = None;

        assert!(ShadowWithdrawWorker::should_throttle_evm_uncertain_recover(&withdraw, Utc::now()));
    }

    #[test]
    fn withdraw_evm_uncertain_manual_review_timeout_marks_error() {
        let mut withdraw = base_withdraw();
        withdraw.broadcast_uncertain_since_at = Some(Utc::now() - chrono::Duration::hours(25));
        withdraw.broadcast_uncertain_reconciled_at = Some(Utc::now() - chrono::Duration::hours(1));

        assert!(ShadowWithdrawWorker::should_mark_evm_uncertain_for_manual_review(
            &withdraw,
            Utc::now()
        ));
    }

    #[test]
    fn withdraw_evm_prebroadcast_nonce_gap_aligns_when_local_nonce_is_far_ahead() {
        assert!(ShadowWithdrawWorker::should_force_align_prebroadcast_nonce_gap(3, 5));
    }

    #[test]
    fn withdraw_evm_prebroadcast_nonce_gap_does_not_align_for_small_gap() {
        assert!(!ShadowWithdrawWorker::should_force_align_prebroadcast_nonce_gap(3, 4));
        assert!(!ShadowWithdrawWorker::should_force_align_prebroadcast_nonce_gap(5, 3));
    }

    #[test]
    fn withdraw_expired_tron_recover_guard_requires_no_broadcast_evidence() {
        let raw_tx_json = expired_tron_raw_tx_json(123);

        assert!(ShadowWithdrawWorker::should_invalidate_expired_tron_raw_for_recover(
            "tron",
            &raw_tx_json,
            false
        ));
        assert!(!ShadowWithdrawWorker::should_invalidate_expired_tron_raw_for_recover(
            "tron",
            &raw_tx_json,
            true
        ));
    }
}
