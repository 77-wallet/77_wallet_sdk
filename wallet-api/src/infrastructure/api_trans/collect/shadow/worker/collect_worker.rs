#![allow(deprecated)]
// collect/shadow/worker/collect_worker.rs

// Architecture Rule:
// - Broadcast success MUST only update last_broadcast_at
// - transaction_time is an irreversible on-chain confirmation fact
// - Only Scanner / Shadow Recovery may write transaction_time
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use tracing::{error, info, warn};
use wallet_chain_interact::{
    BillResourceConsume,
    tron::{
        self,
        operations::{RawTransactionParams, TronTxOperation},
    },
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool,
    entities::{
        api_collect::{ApiCollectEntity, ApiCollectStatus, ErrCode},
        api_resource_delegation::{
            ApiResourceDelegationEntity, ApiResourceDelegationOperationType,
            ApiResourceDelegationRecoverStatus, ApiResourceDelegationResultStatus,
            ApiResourceDelegationSource, NewApiResourceDelegation,
        },
        api_resource_gate::{
            ApiResourceBlockReason, ApiResourceDependencyType, ApiResourceGateResult,
        },
        api_resource_type::ApiResourceType,
        asset_token_key::AssetTokenKey,
    },
    repositories::api_wallet::{
        account::ApiAccountRepo, collect::ApiCollectRepo,
        resource_delegation::ApiResourceDelegationRepo, wallet::ApiWalletRepo,
    },
};
use wallet_transport_backend::request::api_wallet::{
    resource_delegation::{ResourceApplyReq, ResourceType},
    strategy::ChainConfig,
    transaction::TransType,
};
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
    domain::{
        api_wallet::{
            adapter::tx::RawTx, adapter_factory::ApiChainAdapterFactory,
            chain::ApiChainTransDomain, coin::ApiCoinDomain, strategy::StrategyDomain,
            trans::ApiTransDomain,
        },
        chain::adapter::{ChainAdapterFactory, sol_tx::SYSTEM_ACCOUNT_RENT},
    },
    error::{
        business::{
            BusinessError,
            api_wallet::ApiWalletError,
            chain::{ChainError, InsufficientBalanceDetail},
        },
        service::ServiceError,
    },
    infrastructure::api_trans::{
        collect::legacy::AddressLockManager,
        resource_amount::{
            energy_shortfall_to_apply_amounts, parse_resource_delegation_native_trx_units,
        },
        resource_authorization::{
            ResourceDelegationSigner, new_tron_delegate_args, new_tron_undelegate_args,
            resolve_resource_delegation_signer,
        },
        resource_rpc_auth,
    },
    request::api_wallet::trans::{ApiBaseTransferReq, COLLECT_IGNORE_SENDER_RENT_METADATA},
};

/// Shadow Worker Command 结构
/// 只表达："对某个 trade_no 执行某个确定动作"
#[derive(Debug)]
pub enum ShadowCollectCommand {
    /// 评估资源闸门
    EvalResourceGate(String),
    /// 构建交易
    BuildTx(String),
    /// 广播交易
    Broadcast(String),
    /// 恢复交易
    Recover(String),
    /// 执行平台代理资源代理任务，trade_no 是资源任务号
    ExecuteResourceDelegation(String),
}

#[derive(Debug, Clone, Copy)]
struct ResourceGateSnapshot {
    /// BuildTx 前当前归集交易需要的能量
    required_energy: u64,
    /// BuildTx 前当前归集交易需要的带宽
    required_bandwidth: u64,
    /// 子账户当前可直接使用的能量
    available_energy: i64,
    /// 子账户当前可直接使用的带宽
    available_bandwidth: i64,
    /// 当前链上每 1 TRX 可换算的 Energy，用于本地代理时把 energy 缺口换算为 TRX 数量
    energy_price: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceDelegationBlockPath {
    PlatformFallback,
    LocalFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceGateNextStep {
    Release,
    BlockOnPlatform,
    BlockOnLocal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlatformApplyOutcome {
    Accepted(Option<String>),
    Rejected,
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
    const BUILD_503_RETRY_WINDOW_SECS: i64 = 3 * 60;

    fn is_evm_chain_code(chain_code: &str) -> bool {
        chain_code.eq_ignore_ascii_case("eth") || chain_code.eq_ignore_ascii_case("bnb")
    }

    fn tracks_broadcast_uncertain_state(chain_code: &str) -> bool {
        Self::is_evm_chain_code(chain_code) || chain_code.eq_ignore_ascii_case("sol")
    }

    fn should_spend_all_native_collect(chain_code: &str, token_key: &AssetTokenKey) -> bool {
        chain_code.eq_ignore_ascii_case("sol") && token_key.is_native()
    }

    fn sol_token_collect_sender_rent_reserve(
        chain_code: &str,
        token_key: &AssetTokenKey,
    ) -> Result<Decimal, ServiceError> {
        if chain_code.eq_ignore_ascii_case("sol") && token_key.is_contract() {
            return Ok(conversion::decimal_from_str(&SYSTEM_ACCOUNT_RENT.to_string())?);
        }

        Ok(Decimal::ZERO)
    }

    fn collect_balance_need(
        fee: Decimal,
        value: Decimal,
        spend_all_native: bool,
    ) -> Result<Decimal, ServiceError> {
        if spend_all_native { Ok(fee) } else { Ok(fee + value) }
    }

    fn is_collect_amount_shortage(balance: &str, value: &str) -> Result<bool, ServiceError> {
        let balance = conversion::decimal_from_str(balance)?;
        let value = conversion::decimal_from_str(value)?;
        Ok(balance < value)
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

    fn broadcast_uncertain_elapsed_secs(req: &ApiCollectEntity, now: DateTime<Utc>) -> Option<i64> {
        req.broadcast_uncertain_since_at
            .map(|since| now.signed_duration_since(since).num_seconds().max(0))
    }

    fn should_auto_fail_broadcast_uncertain(req: &ApiCollectEntity, now: DateTime<Utc>) -> bool {
        Self::broadcast_uncertain_elapsed_secs(req, now)
            .is_some_and(|elapsed| elapsed >= Self::EVM_UNCERTAIN_TIMEOUT_SECS)
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

    fn is_collect_build_503_error(err: &ServiceError) -> bool {
        err.to_string().contains("code=503")
    }

    fn build_503_elapsed_secs(req: &ApiCollectEntity, now: DateTime<Utc>) -> Option<i64> {
        req.updated_at.map(|updated_at| now.signed_duration_since(updated_at).num_seconds().max(0))
    }

    fn should_terminal_fail_collect_build_503(
        req: &ApiCollectEntity,
        err: &ServiceError,
        now: DateTime<Utc>,
    ) -> bool {
        if !Self::is_collect_build_503_error(err) {
            return false;
        }

        if Self::build_503_elapsed_secs(req, now)
            .is_some_and(|elapsed| elapsed >= Self::BUILD_503_RETRY_WINDOW_SECS)
        {
            return true;
        }

        false
    }

    fn tron_resource_ready(
        available_energy: i64,
        _available_bandwidth: i64,
        required_energy: u64,
        _required_bandwidth: u64,
    ) -> bool {
        // The collect resource gate only owns Energy delegation. Bandwidth
        // shortage is paid/bypassed by the existing collect main flow, so it
        // must not trigger `/resourceDl/apply` for an Energy task.
        available_energy >= required_energy as i64
    }

    fn collect_local_delegate_trade_no(origin_trade_no: &str) -> String {
        // 本地代理是 SDK 自己执行的 fallback 任务，所以需要确定性任务号；
        // 否则 collect 每次重新评估时都会重复创建本地代理任务。
        format!("rsc_local_delegate_{}", origin_trade_no)
    }

    fn resource_shortfall(required: u64, available: i64) -> u64 {
        required.saturating_sub(available.max(0) as u64)
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

    fn should_reopen_fee_cycle_for_solana_token_collect(
        req: &ApiCollectEntity,
        err: &ServiceError,
    ) -> bool {
        req.chain_code.eq_ignore_ascii_case("sol")
            && req.token_addr.is_contract()
            && Self::is_solana_rent_exempt_reserve_balance_error(req, err)
    }

    fn should_terminal_fail_solana_token_collect_rent_shortage(
        req: &ApiCollectEntity,
        err: &ServiceError,
    ) -> bool {
        Self::should_reopen_fee_cycle_for_solana_token_collect(req, err)
            && req.service_fee_uploaded_at.is_some()
    }

    fn should_terminal_fail_solana_token_collect_fee_shortage_after_completed_fee_cycle(
        req: &ApiCollectEntity,
    ) -> bool {
        req.chain_code.eq_ignore_ascii_case("sol")
            && req.token_addr.is_contract()
            && req.service_fee_uploaded_at.is_some()
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

    fn is_collect_amount_insufficient_error(err: &ServiceError) -> bool {
        match err {
            ServiceError::Business(crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::InsufficientBalance(detail),
            )) => detail
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("collect amount is insufficient")),
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
            ShadowCollectCommand::EvalResourceGate(trade_no) => trade_no,
            ShadowCollectCommand::BuildTx(trade_no) => trade_no,
            ShadowCollectCommand::Broadcast(trade_no) => trade_no,
            ShadowCollectCommand::Recover(trade_no) => trade_no,
            ShadowCollectCommand::ExecuteResourceDelegation(trade_no) => trade_no,
        };

        info!(trade_no = %trade_no, command = ?cmd, source = "shadow_worker_v2", "Received shadow collect command");

        match cmd {
            ShadowCollectCommand::EvalResourceGate(trade_no) => {
                self.process_resource_gate(trade_no).await
            }
            ShadowCollectCommand::BuildTx(trade_no) => self.process_build_tx(trade_no).await,
            ShadowCollectCommand::Broadcast(trade_no) => self.process_broadcast(trade_no).await,
            ShadowCollectCommand::Recover(trade_no) => self.process_recover(trade_no).await,
            ShadowCollectCommand::ExecuteResourceDelegation(trade_no) => {
                let result = self.process_resource_delegation_execute(trade_no.clone()).await;
                self.handle_resource_delegation_terminal_failure_if_needed(&trade_no, result).await
            }
        }
    }

    async fn handle_resource_delegation_terminal_failure_if_needed(
        &self,
        resource_trade_no: &str,
        result: Result<(), ServiceError>,
    ) -> Result<(), ServiceError> {
        let Err(err) = result else {
            return Ok(());
        };

        match err.retry_policy() {
            wallet_utils::RetryPolicy::Never => {
                self.mark_resource_delegation_failed(resource_trade_no, &err).await?;
                self.release_collect_gate_after_local_delegation_failure(resource_trade_no).await?;
                Ok(())
            }
            wallet_utils::RetryPolicy::Delay => {
                self.schedule_resource_delegation_rebuild_retry(resource_trade_no, &err).await?;
                info!(
                    resource_trade_no = %resource_trade_no,
                    error = %err,
                    source = "shadow_worker_v2",
                    "Resource delegation terminal step failed, will retry later"
                );
                Ok(())
            }
        }
    }

    fn resource_delegation_retry_wait_secs(retry_count: i64) -> i64 {
        let exponent = retry_count.clamp(0, 6) as u32;
        (60_i64 * (1_i64 << exponent)).min(3600)
    }

    async fn schedule_resource_delegation_rebuild_retry(
        &self,
        resource_trade_no: &str,
        err: &ServiceError,
    ) -> Result<(), ServiceError> {
        let task = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.collect_pool,
            resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        let wait_secs = Self::resource_delegation_retry_wait_secs(task.retry_count);
        let next_retry_at = Utc::now() + chrono::Duration::seconds(wait_secs);
        let next_status = if err.is_network_error() {
            ApiResourceDelegationRecoverStatus::RetryBuild
        } else {
            ApiResourceDelegationRecoverStatus::RetryRecover
        };

        let affected = ApiResourceDelegationRepo::reset_for_retry(
            &self.collect_pool,
            resource_trade_no,
            next_status,
            &next_retry_at.to_rfc3339(),
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        info!(
            resource_trade_no = %resource_trade_no,
            origin_trade_no = ?task.origin_trade_no,
            affected = %affected,
            retry_count = task.retry_count + 1,
            recover_status = ?next_status,
            next_retry_at = %next_retry_at.to_rfc3339(),
            wait_secs,
            error = %err,
            source = "shadow_worker_v2",
            "Resource delegation reset for retry"
        );

        Ok(())
    }

    async fn process_resource_delegation_execute(
        &self,
        resource_trade_no: String,
    ) -> Result<(), ServiceError> {
        info!(
            resource_trade_no = %resource_trade_no,
            source = "shadow_worker_v2",
            "Processing resource delegation execution"
        );

        let affected =
            ApiResourceDelegationRepo::claim_build_slot(&self.collect_pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            info!(
                resource_trade_no = %resource_trade_no,
                source = "shadow_worker_v2",
                "Resource delegation execution was already claimed or completed"
            );
            return Ok(());
        }

        let delegation = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.collect_pool,
            &resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        if delegation.tx_hash.is_some() {
            info!(
                resource_trade_no = %resource_trade_no,
                source = "shadow_worker_v2",
                "Resource delegation already has tx_hash, skipping execution"
            );
            return Ok(());
        }

        let tx_hash = self.execute_tron_resource_delegation(&delegation).await?;
        let affected = ApiResourceDelegationRepo::mark_broadcast_success(
            &self.collect_pool,
            &resource_trade_no,
            &tx_hash,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        if affected == 0 {
            info!(
                resource_trade_no = %resource_trade_no,
                tx_hash = %tx_hash,
                source = "shadow_worker_v2",
                "Resource delegation broadcast fact was already committed"
            );
        } else {
            info!(
                resource_trade_no = %resource_trade_no,
                tx_hash = %tx_hash,
                source = "shadow_worker_v2",
                "Resource delegation broadcast fact committed"
            );
        }

        self.release_collect_gate_after_local_delegation_success(&delegation).await?;

        Ok(())
    }

    async fn execute_tron_resource_delegation(
        &self,
        delegation: &ApiResourceDelegationEntity,
    ) -> Result<String, ServiceError> {
        let mut auth_retry_attempted = false;
        loop {
            match self.execute_tron_resource_delegation_once(delegation).await {
                Ok(tx_hash) => return Ok(tx_hash),
                Err(err)
                    if !auth_retry_attempted
                        && resource_rpc_auth::should_retry_after_rpc_auth_error(&err) =>
                {
                    auth_retry_attempted = true;
                    resource_rpc_auth::refresh_and_prepare_retry(
                        &delegation.chain_code,
                        "collect_resource_delegation",
                        &delegation.resource_trade_no,
                        &err,
                    )
                    .await?;
                }
                Err(err) => return Err(err),
            }
        }
    }

    async fn execute_tron_resource_delegation_once(
        &self,
        delegation: &ApiResourceDelegationEntity,
    ) -> Result<String, ServiceError> {
        if !delegation.chain_code.eq_ignore_ascii_case("tron") {
            return Err(ServiceError::Parameter(format!(
                "resource delegation only supports tron, got {}",
                delegation.chain_code
            )));
        }

        let trx_amount = parse_resource_delegation_native_trx_units(&delegation.native_amount)?;
        let resource = Self::tron_resource_name(delegation.resource_type);
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        let _chain_rpc_guard =
            crate::infrastructure::chain_rpc_guard::acquire_if_guarded(&delegation.chain_code)
                .await;
        let signer = resolve_resource_delegation_signer(delegation).await?;

        let raw = match delegation.operation_type {
            ApiResourceDelegationOperationType::Delegate => {
                let args = new_tron_delegate_args(
                    &delegation.owner_address,
                    &delegation.receiver_address,
                    trx_amount,
                    resource,
                    signer.permission_id,
                )?;
                args.build_raw_transaction(chain.get_provider()).await?
            }
            ApiResourceDelegationOperationType::Undelegate => {
                let args = new_tron_undelegate_args(
                    &delegation.owner_address,
                    &delegation.receiver_address,
                    trx_amount,
                    resource,
                    signer.permission_id,
                )?;
                args.build_raw_transaction(chain.get_provider()).await?
            }
        };
        let (tx_hash, raw_tx) =
            self.sign_tron_resource_delegation(delegation, &signer, raw).await?;
        let tx_resp =
            ApiTransDomain::broadcast_transfer(&delegation.chain_code, raw_tx, Some(&tx_hash))
                .await?;

        let Some(tx) = tx_resp else {
            info!(
                resource_trade_no = %delegation.resource_trade_no,
                tx_hash = %tx_hash,
                source = "shadow_worker_v2",
                "Resource delegation broadcast result uncertain"
            );
            return Err(ServiceError::Parameter(
                "resource delegation broadcast result uncertain".to_string(),
            ));
        };

        if tx.tx_hash != tx_hash {
            error!(
                resource_trade_no = %delegation.resource_trade_no,
                expected_tx_hash = %tx_hash,
                broadcast_tx_hash = %tx.tx_hash,
                source = "shadow_worker_v2",
                "Resource delegation tx_hash mismatch between build and broadcast"
            );
            return Err(ServiceError::System(SystemError::Internal(
                "resource delegation tx_hash mismatch between build and broadcast".to_string(),
            )));
        }

        Ok(tx_hash)
    }

    async fn sign_tron_resource_delegation(
        &self,
        delegation: &ApiResourceDelegationEntity,
        signer: &ResourceDelegationSigner,
        mut raw: RawTransactionParams,
    ) -> Result<(String, RawTx), ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        let provider = chain.get_provider();
        let consumer =
            provider.transfer_fee(&delegation.owner_address, None, &raw.raw_data_hex, 1).await?;
        let balance = chain.balance(&delegation.owner_address, None).await?;
        if balance.to::<i64>() < consumer.transaction_fee_i64() {
            return Err(ServiceError::Parameter(format!(
                "resource delegation balance is insufficient for tx fee: balance={}, need={}",
                balance,
                consumer.transaction_fee_i64()
            )));
        }

        let handles = crate::context::get_context()?.get_handles_arc().await?;
        let private_key_manager = handles.get_global_private_key_manager();
        let private_key = private_key_manager
            .get_private_key(&signer.signer_address, &delegation.chain_code)
            .await?;
        let sign = wallet_utils::sign::sign_tron(&raw.tx_id, &private_key, None)?;
        raw.signature.push(sign);

        let tx_hash = raw.tx_id.clone();
        let raw_tx = RawTx::Tron(
            raw,
            BillResourceConsume::new_tron(consumer.act_bandwidth() as u64, 0),
            consumer.transaction_fee(),
        );

        Ok((tx_hash, raw_tx))
    }

    fn tron_resource_name(resource_type: ApiResourceType) -> &'static str {
        match resource_type {
            ApiResourceType::Bandwidth => "bandwidth",
            ApiResourceType::Energy => "energy",
        }
    }

    async fn mark_resource_delegation_failed(
        &self,
        resource_trade_no: &str,
        err: &ServiceError,
    ) -> Result<(), ServiceError> {
        let (err_code, err_msg) = Self::resource_delegation_failure_fact_from_error(err);
        let affected = ApiResourceDelegationRepo::mark_failed_if_unfinished(
            &self.collect_pool,
            resource_trade_no,
            &err_code,
            &err_msg,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        if affected == 0 {
            info!(
                resource_trade_no = %resource_trade_no,
                err_code = %err_code,
                source = "shadow_worker_v2",
                "Resource delegation failure fact already committed or no longer eligible"
            );
        } else {
            info!(
                resource_trade_no = %resource_trade_no,
                err_code = %err_code,
                source = "shadow_worker_v2",
                "Resource delegation failure fact committed"
            );
        }

        Ok(())
    }

    fn resource_delegation_failure_fact_from_error(err: &ServiceError) -> (String, String) {
        let err_code = if err.is_network_error() { "ERR_6005" } else { "ERR_6008" };
        (err_code.to_string(), err.to_string())
    }

    /// 执行资源闸门检查，入参是原归集订单号，不是资源任务号。
    ///
    /// 边界说明：
    /// - 这里的“评估资源闸门”是一个真实操作步骤
    /// - `resource_ready` / `need_platform_delegate` 只是这一步落下的结果事实
    /// - worker 负责提交事实，不负责继续调度；后续推进由 scanner/advancer 基于事实完成
    async fn process_resource_gate(&self, origin_trade_no: String) -> Result<(), ServiceError> {
        info!(origin_trade_no = %origin_trade_no, source = "shadow_worker_v2", "Processing resource gate command");

        if let Err(err) = self.process_resource_gate_inner(&origin_trade_no).await {
            error!(origin_trade_no = %origin_trade_no, error = %err, source = "shadow_worker_v2", "Resource gate check failed");
            return Err(err);
        }

        Ok(())
    }

    async fn process_resource_gate_inner(&self, origin_trade_no: &str) -> Result<(), ServiceError> {
        let req = self.get_collect_entity(origin_trade_no).await?;

        if !Self::is_tron_collect(&req.chain_code) {
            return Ok(());
        }

        if Self::resource_gate_already_resolved(&req) {
            info!(
                origin_trade_no = %origin_trade_no,
                source = "shadow_worker_v2",
                "Resource gate already resolved or collect is no longer eligible"
            );
            return Ok(());
        }

        let exec_to_addr = self.resolve_collect_to_addr(&req).await?;
        let snapshot = self.eval_collect_resource_gate_snapshot(&req, &exec_to_addr).await?;

        // 资源顺序只处理到“允许回到旧 collect 主链”为止：
        // 自身资源 -> 平台代理 -> 本地代理 fallback -> release gate。
        // release 之后，主币不足 / 补币 / 原失败收口仍走上一版已经稳定的旧闭环。
        let next_step = self
            .decide_collect_resource_gate_next_step(&req, snapshot.clone(), origin_trade_no)
            .await?;
        self.apply_collect_resource_gate_next_step(
            next_step,
            origin_trade_no,
            &req,
            &exec_to_addr,
            snapshot,
        )
        .await?;

        info!(
            origin_trade_no,
            required_energy = %snapshot.required_energy,
            available_energy = %snapshot.available_energy,
            required_bandwidth = %snapshot.required_bandwidth,
            available_bandwidth = %snapshot.available_bandwidth,
            source = "shadow_worker_v2",
            "TRON collect resource gate evaluated"
        );

        Ok(())
    }

    fn is_tron_collect(chain_code: &str) -> bool {
        chain_code.eq_ignore_ascii_case("tron")
    }

    fn resource_gate_already_resolved(req: &ApiCollectEntity) -> bool {
        req.resource_gate_released_at.is_some()
            || req.raw_tx.is_some()
            || req.transaction_time.is_some()
            || req.finished_at.is_some()
            || req.err_code.is_some()
    }

    async fn process_release_resource_gate(
        &self,
        origin_trade_no: String,
    ) -> Result<(), ServiceError> {
        let req = self.get_collect_entity(&origin_trade_no).await?;
        if !Self::is_tron_collect(&req.chain_code) {
            return Ok(());
        }
        if req.resource_gate_released_at.is_some() {
            info!(
                origin_trade_no = %origin_trade_no,
                source = "shadow_worker_v2",
                "Resource gate already released"
            );
            return Ok(());
        }
        if req.raw_tx.is_some()
            || req.transaction_time.is_some()
            || req.finished_at.is_some()
            || req.err_code.is_some()
        {
            info!(
                origin_trade_no = %origin_trade_no,
                source = "shadow_worker_v2",
                "Skip release resource gate: collect no longer eligible"
            );
            return Ok(());
        }

        // The resource gate is a pre-BuildTx fact. Releasing it only records
        // that the collect order may enter the existing BuildTx flow; raw_tx
        // and tx_hash are still produced by the normal build path.
        let rows = ApiCollectRepo::mark_resource_released(
            &self.collect_pool,
            &origin_trade_no,
            ApiResourceGateResult::ResourceReady,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        info!(
            origin_trade_no = %origin_trade_no,
            rows = %rows,
            source = "shadow_worker_v2",
            "TRON collect resource gate released"
        );
        self.advancer.try_advance(&origin_trade_no).await;
        Ok(())
    }

    async fn process_block_on_platform_delegation(
        &self,
        origin_trade_no: String,
    ) -> Result<(), ServiceError> {
        let req = self.get_collect_entity(&origin_trade_no).await?;
        if !Self::is_tron_collect(&req.chain_code) {
            return Ok(());
        }
        if Self::resource_gate_already_resolved(&req) {
            info!(
                origin_trade_no = %origin_trade_no,
                source = "shadow_worker_v2",
                "Skip block on platform delegation: resource gate already resolved"
            );
            return Ok(());
        }

        let exec_to_addr = self.resolve_collect_to_addr(&req).await?;
        let main_coin = ApiChainTransDomain::main_coin(&req.chain_code).await?;
        let (token_symbol, token_key, token_decimals) = if req.token_addr.is_contract() {
            let token_coin =
                ApiCoinDomain::get_coin_by_token_key_exact(&req.chain_code, req.token_addr.clone())
                    .await?;
            (token_coin.symbol, token_coin.token_address, token_coin.decimals)
        } else {
            (main_coin.symbol.clone(), AssetTokenKey::Native, main_coin.decimals)
        };
        let fee_details = self
            .estimate_tron_fee_details(
                &req.from_addr,
                &exec_to_addr,
                &req.value,
                &token_symbol,
                &main_coin.symbol,
                token_key,
                token_decimals,
            )
            .await?;
        let adapter = ApiChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;
        let resource = adapter.account_resource(&req.from_addr).await?;
        let available_energy = resource.available_energy();
        let available_bandwidth = resource.available_bandwidth();
        let energy_price = resource.energy_price();

        self.commit_platform_delegation_block(
            &origin_trade_no,
            &req,
            &exec_to_addr,
            fee_details.energy,
            fee_details.bandwidth,
            available_energy,
            available_bandwidth,
            energy_price,
        )
        .await
    }

    async fn eval_collect_resource_gate_snapshot(
        &self,
        req: &ApiCollectEntity,
        exec_to_addr: &str,
    ) -> Result<ResourceGateSnapshot, ServiceError> {
        // 这里只做“评估快照”：
        // - 估算如果现在 BuildTx，需要多少资源
        // - 读取子账户当前链上资源余额
        // 不在这里决定走平台代理还是本地代理，也不直接写 blocked/released 事实。
        let main_coin = ApiChainTransDomain::main_coin(&req.chain_code).await?;
        let (token_symbol, token_key, token_decimals) = if req.token_addr.is_contract() {
            let token_coin =
                ApiCoinDomain::get_coin_by_token_key_exact(&req.chain_code, req.token_addr.clone())
                    .await?;
            (token_coin.symbol, token_coin.token_address, token_coin.decimals)
        } else {
            (main_coin.symbol.clone(), AssetTokenKey::Native, main_coin.decimals)
        };

        let fee_details = self
            .estimate_tron_fee_details(
                &req.from_addr,
                exec_to_addr,
                &req.value,
                &token_symbol,
                &main_coin.symbol,
                token_key,
                token_decimals,
            )
            .await?;
        let adapter = ApiChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;
        let resource = adapter.account_resource(&req.from_addr).await?;

        Ok(ResourceGateSnapshot {
            required_energy: fee_details.energy,
            required_bandwidth: fee_details.bandwidth,
            available_energy: resource.available_energy(),
            available_bandwidth: resource.available_bandwidth(),
            energy_price: resource.energy_price(),
        })
    }

    async fn decide_collect_resource_gate_next_step(
        &self,
        req: &ApiCollectEntity,
        snapshot: ResourceGateSnapshot,
        origin_trade_no: &str,
    ) -> Result<ResourceGateNextStep, ServiceError> {
        if Self::tron_resource_ready(
            snapshot.available_energy,
            snapshot.available_bandwidth,
            snapshot.required_energy,
            snapshot.required_bandwidth,
        ) {
            return Ok(ResourceGateNextStep::Release);
        }

        let delegations =
            ApiResourceDelegationRepo::list_by_origin_trade_no(&self.collect_pool, origin_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
        let next_step = match Self::decide_collect_resource_block_path(req, &delegations) {
            ResourceDelegationBlockPath::LocalFallback => ResourceGateNextStep::BlockOnLocal,
            ResourceDelegationBlockPath::PlatformFallback => ResourceGateNextStep::BlockOnPlatform,
        };

        Ok(next_step)
    }

    /// Main collect resource-gate skeleton:
    /// 1. evaluate current resource snapshot
    /// 2. decide the next step from facts
    /// 3. commit only the facts for that one step
    async fn apply_collect_resource_gate_next_step(
        &self,
        next_step: ResourceGateNextStep,
        origin_trade_no: &str,
        req: &ApiCollectEntity,
        exec_to_addr: &str,
        snapshot: ResourceGateSnapshot,
    ) -> Result<(), ServiceError> {
        match next_step {
            ResourceGateNextStep::Release => {
                self.process_release_resource_gate(origin_trade_no.to_string()).await
            }
            ResourceGateNextStep::BlockOnPlatform => {
                self.commit_platform_delegation_block(
                    origin_trade_no,
                    req,
                    exec_to_addr,
                    snapshot.required_energy,
                    snapshot.required_bandwidth,
                    snapshot.available_energy,
                    snapshot.available_bandwidth,
                    snapshot.energy_price,
                )
                .await
            }
            ResourceGateNextStep::BlockOnLocal => {
                self.commit_local_delegation_block(
                    origin_trade_no,
                    req,
                    exec_to_addr,
                    snapshot.required_energy,
                    snapshot.available_energy,
                    snapshot.energy_price,
                )
                .await
            }
        }
    }

    fn decide_collect_resource_block_path(
        req: &ApiCollectEntity,
        delegations: &[ApiResourceDelegationEntity],
    ) -> ResourceDelegationBlockPath {
        // 文档顺序要求：
        // 子账户自身能量 -> 平台资源代理 -> 出款地址本地代理 -> 后续主链
        //
        // 所以只要平台代理已经失败，或者本地 fallback 已经存在，
        // 下一次 EvalResourceGate 就必须把 blocked 事实切到 local_delegate。
        let has_local_fallback = delegations.iter().any(|delegation| {
            delegation.source == ApiResourceDelegationSource::Local
                && delegation.operation_type == ApiResourceDelegationOperationType::Delegate
        });
        let platform_failed = delegations.iter().any(|delegation| {
            delegation.source == ApiResourceDelegationSource::Platform
                && delegation.operation_type == ApiResourceDelegationOperationType::Delegate
                && (delegation.err_code.is_some()
                    || matches!(delegation.tx_status.as_deref(), Some("fail")))
        });

        if req.resource_block_reason == Some(ApiResourceBlockReason::NeedLocalDelegate)
            || has_local_fallback
            || platform_failed
        {
            ResourceDelegationBlockPath::LocalFallback
        } else {
            ResourceDelegationBlockPath::PlatformFallback
        }
    }

    async fn commit_platform_delegation_block(
        &self,
        origin_trade_no: &str,
        req: &ApiCollectEntity,
        exec_to_addr: &str,
        required_energy: u64,
        required_bandwidth: u64,
        available_energy: i64,
        available_bandwidth: i64,
        energy_price: f64,
    ) -> Result<(), ServiceError> {
        let amount = Self::resource_shortfall(required_energy, available_energy).max(1).to_string();
        let resource_trade_no = match self
            .apply_platform_resource_delegation(
                &req.uid,
                origin_trade_no,
                &req.chain_code,
                &req.from_addr,
                &amount,
            )
            .await?
        {
            PlatformApplyOutcome::Accepted(resource_trade_no) => resource_trade_no,
            PlatformApplyOutcome::Rejected => {
                return self
                    .commit_local_delegation_block(
                        origin_trade_no,
                        req,
                        &exec_to_addr,
                        required_energy,
                        available_energy,
                        energy_price,
                    )
                    .await;
            }
        };

        // 商户侧只记录“原单正在等待平台代理结果”这个事实。
        // 真正的平台代理订单由平台钱包接收并执行，不在商户钱包本地创建任务行。
        let rows = ApiCollectRepo::mark_resource_blocked(
            &self.collect_pool,
            origin_trade_no,
            ApiResourceBlockReason::NeedPlatformDelegate,
            resource_trade_no.as_deref(),
            Some(ApiResourceDependencyType::PlatformDelegate),
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        info!(
            origin_trade_no = %origin_trade_no,
            rows = %rows,
            required_energy = %required_energy,
            available_energy = %available_energy,
            required_bandwidth = %required_bandwidth,
            available_bandwidth = %available_bandwidth,
            resource_trade_no = ?resource_trade_no,
            source = "shadow_worker_v2",
            "TRON collect resource gate blocked"
        );

        Ok(())
    }

    async fn apply_platform_resource_delegation(
        &self,
        uid: &str,
        origin_trade_no: &str,
        chain_code: &str,
        receiver_address: &str,
        amount: &str,
    ) -> Result<PlatformApplyOutcome, ServiceError> {
        let adapter = ApiChainAdapterFactory::get_transaction_adapter(chain_code).await?;
        let resource = adapter.account_resource(receiver_address).await?;
        let amounts = energy_shortfall_to_apply_amounts(amount, resource.energy_price())?;

        let wallet = ApiWalletRepo::find_by_uid(&self.core_pool, uid)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;

        let app_id = wallet.as_ref().and_then(|w| w.app_id.as_deref()).unwrap_or(uid);
        let org_id = wallet.as_ref().and_then(|w| w.merchant_id.as_deref()).unwrap_or(uid);

        let req = ResourceApplyReq::new(
            origin_trade_no,
            app_id,
            org_id,
            Some(chain_code),
            amounts.native_token_amount,
            Some(amounts.resource_amount),
            ResourceType::Energy,
            receiver_address,
            TransType::Col,
        );

        tracing::info!(
            origin_trade_no = %origin_trade_no,
            req = ?req,
            source = "shadow_worker_v2",
            "Platform resource delegation apply request"
        );
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let resp = backend_api.apply_resource_delegation(&req).await?;

        if resp.is_success() {
            info!(
                origin_trade_no = %origin_trade_no,
                resource_trade_no = ?resp.dl_trade_no,
                source = "shadow_worker_v2",
                "Platform resource delegation apply succeeded"
            );
            Ok(PlatformApplyOutcome::Accepted(resp.dl_trade_no))
        } else {
            warn!(
                origin_trade_no = %origin_trade_no,
                source = "shadow_worker_v2",
                "Platform resource delegation apply rejected, will try alternative paths"
            );
            Ok(PlatformApplyOutcome::Rejected)
        }
    }

    async fn commit_local_delegation_block(
        &self,
        origin_trade_no: &str,
        req: &ApiCollectEntity,
        exec_to_addr: &str,
        required_energy: u64,
        available_energy: i64,
        energy_price: f64,
    ) -> Result<(), ServiceError> {
        // local delegation 的 owner 是出款地址，receiver 是当前待归集子地址。
        // 这里写下的是“本地代理 fallback 已成为当前依赖”的事实，
        // 不是说本地代理已经执行成功。
        //
        // 一旦 local delegation 到终态，collect 只会被放回旧主链入口，
        // 不会在资源链里继续承接主币/补币逻辑。
        let resource_trade_no = Self::collect_local_delegate_trade_no(origin_trade_no);
        let amount = Self::resource_shortfall(required_energy, available_energy).max(1).to_string();
        let native_amount = energy_shortfall_to_apply_amounts(&amount, energy_price)?
            .native_token_amount
            .to_string();
        let delegation = NewApiResourceDelegation::local_delegate(
            req.uid.clone(),
            resource_trade_no.clone(),
            req.trade_no.clone(),
            i64::from(req.trade_type),
            exec_to_addr.to_string(),
            req.from_addr.clone(),
            native_amount,
            amount,
        );
        ApiResourceDelegationRepo::upsert(&self.collect_pool, delegation)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        let rows = ApiCollectRepo::mark_resource_blocked(
            &self.collect_pool,
            origin_trade_no,
            ApiResourceBlockReason::NeedLocalDelegate,
            Some(&resource_trade_no),
            Some(ApiResourceDependencyType::LocalDelegate),
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        info!(
            origin_trade_no = %origin_trade_no,
            rows = %rows,
            resource_trade_no = %resource_trade_no,
            owner_address = %exec_to_addr,
            receiver_address = %req.from_addr,
            source = "shadow_worker_v2",
            "TRON collect resource gate switched to local delegation fallback"
        );
        Ok(())
    }

    async fn release_collect_gate_after_local_delegation_success(
        &self,
        delegation: &ApiResourceDelegationEntity,
    ) -> Result<(), ServiceError> {
        if delegation.source != ApiResourceDelegationSource::Local
            || delegation.operation_type != ApiResourceDelegationOperationType::Delegate
        {
            return Ok(());
        }
        ApiResourceDelegationRepo::mark_result_received(
            &self.collect_pool,
            &delegation.resource_trade_no,
            ApiResourceDelegationResultStatus::Success,
            None,
            None,
            None,
            None,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        let Some(origin_trade_no) = delegation.origin_trade_no.as_deref() else {
            return Ok(());
        };
        self.release_collect_gate_after_local_delegation(
            origin_trade_no,
            ApiResourceGateResult::LocalDelegationSuccess,
        )
        .await
    }

    async fn release_collect_gate_after_local_delegation_failure(
        &self,
        resource_trade_no: &str,
    ) -> Result<(), ServiceError> {
        let delegation = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.collect_pool,
            resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        if delegation.source != ApiResourceDelegationSource::Local {
            return Ok(());
        }
        if delegation.operation_type != ApiResourceDelegationOperationType::Delegate {
            return Ok(());
        }
        let _ = ApiResourceDelegationRepo::mark_result_received(
            &self.collect_pool,
            &delegation.resource_trade_no,
            ApiResourceDelegationResultStatus::Fail,
            None,
            delegation.err_code.as_deref(),
            delegation.err_msg.as_deref(),
            None,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        let Some(origin_trade_no) = delegation.origin_trade_no.as_deref() else {
            return Ok(());
        };
        self.release_collect_gate_after_local_delegation(
            origin_trade_no,
            ApiResourceGateResult::LocalDelegationFailedBypass,
        )
        .await
    }

    async fn release_collect_gate_after_local_delegation(
        &self,
        origin_trade_no: &str,
        gate_result: ApiResourceGateResult,
    ) -> Result<(), ServiceError> {
        // local delegation 到达终态后，collect 不再卡在资源 gate。
        // 后面是否还会因为主币不足、服务费不足而停下，交回原有 BuildTx/fee 流程判断。
        let affected = ApiCollectRepo::mark_resource_released(
            &self.collect_pool,
            origin_trade_no,
            gate_result,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            info!(
                origin_trade_no = %origin_trade_no,
                ?gate_result,
                source = "shadow_worker_v2",
                "Collect gate already released after local delegation"
            );
        }
        self.advancer.try_advance(origin_trade_no).await;
        Ok(())
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
                let elapsed = Self::broadcast_uncertain_elapsed_secs(&req, now).unwrap_or_default();
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
                if !Self::tracks_broadcast_uncertain_state(&req.chain_code) {
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
                    "Broadcast uncertain state recorded"
                );

                let elapsed_secs =
                    Self::broadcast_uncertain_elapsed_secs(&refreshed, now).unwrap_or_default();
                let timed_out = Self::should_auto_fail_broadcast_uncertain(&refreshed, now);
                if !timed_out {
                    return Ok(());
                }

                if !Self::is_evm_chain_code(&refreshed.chain_code) {
                    warn!(
                        trade_no = %refreshed.trade_no,
                        tx_hash = %refreshed.tx_hash.as_deref().unwrap_or_default(),
                        uncertain_duration_sec = elapsed_secs,
                        source = "shadow_worker_v2",
                        "SOL uncertain timeout reached; auto fail order"
                    );

                    let rows_affected = ApiCollectRepo::update_api_collect_status_and_err(
                        &self.collect_pool,
                        &refreshed.trade_no,
                        ApiCollectStatus::SendingTxFailed,
                        Self::EVM_UNCERTAIN_AUTO_FAIL_ERR_CODE,
                        "SOL broadcast uncertain timeout after 5m; confirmed result still not visible",
                    )
                    .await
                    .map_err(|db_err: wallet_database::Error| {
                        error!(trade_no = %refreshed.trade_no, error = %db_err, source = "shadow_worker_v2", "Failed to mark SOL uncertain timeout auto-fail");
                        ServiceError::Database(db_err.into())
                    })?;
                    if rows_affected > 0 {
                        self.advancer.try_advance(&refreshed.trade_no).await;
                    }
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
                    let rows = ApiCollectRepo::invalidate_raw_tx_for_rebroadcast(
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
        if !self.check_collect_amount(&req).await? {
            info!(
                trade_no = %trade_no,
                reason_code = "amount_check_failed",
                source = "shadow_worker_v2",
                "Collect amount insufficient, failing build directly"
            );
            return Err(ServiceError::Business(BusinessError::Chain(
                ChainError::InsufficientBalance(
                    InsufficientBalanceDetail::new()
                        .from_addr(req.from_addr.clone())
                        .to_addr(req.to_addr.clone())
                        .chain_code(req.chain_code.clone())
                        .token_addr(req.token_addr.to_string())
                        .value(req.value.clone())
                        .reason("collect amount is insufficient; balance is below requested value"),
                ),
            )));
        }

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

                if Self::tracks_broadcast_uncertain_state(&req.chain_code) {
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
                        "Broadcast uncertain state recorded"
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
        let sender_rent_reserve =
            Self::sol_token_collect_sender_rent_reserve(&req.chain_code, &token_key)?;
        let fee_str = match self
            .estimate_fee(
                &req.from_addr,
                &req.to_addr,
                &req.value,
                chain_code,
                &token_symbol,
                &main_coin.symbol,
                token_key.clone(),
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
            Err(err) if Self::should_reopen_fee_cycle_for_solana_token_collect(&req, &err) => {
                if Self::should_terminal_fail_solana_token_collect_rent_shortage(&req, &err) {
                    tracing::warn!(
                        trade_no = %req.trade_no,
                        from_addr = %req.from_addr,
                        to_addr = %req.to_addr,
                        chain_code = %req.chain_code,
                        token_addr = %req.token_addr,
                        source = "shadow_worker_v2",
                        "Fee estimation reported Solana sender rent reserve shortage after completed fee cycle; failing build"
                    );
                    return Err(err);
                }

                tracing::warn!(
                    trade_no = %req.trade_no,
                    from_addr = %req.from_addr,
                    to_addr = %req.to_addr,
                    chain_code = %req.chain_code,
                    token_addr = %req.token_addr,
                    source = "shadow_worker_v2",
                    "Fee estimation reported Solana sender rent reserve shortage; reopening service fee cycle"
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
            // Solana token collect 需要保留 sender 的 rent-exempt reserve。
            tracing::info!(
                trade_no = %req.trade_no,
                source = "shadow_worker_v2",
                sender_rent_reserve = %sender_rent_reserve,
                "collect_tx:send: 代币交易，手续费检查需要额外保留 sender rent reserve"
            );
            fee + sender_rent_reserve
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
            // 由 caller 进入后续的 recover / side-effect 流程处理手续费补单。
            Ok(false)
        } else {
            tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 手续费充足，继续交易");
            Ok(true)
        }
    }

    /// 检查可归集金额是否充足。
    ///
    /// 返回值语义：
    /// - Ok(true): 可归集金额充足
    /// - Ok(false): 可归集金额不足，caller 必须直接失败，不再进入 fee 逻辑
    /// - Err(_): 基础设施错误
    pub(crate) async fn check_collect_amount(
        &self,
        req: &ApiCollectEntity,
    ) -> Result<bool, ServiceError> {
        let chain_code: ChainCode = req.chain_code.as_str().try_into()?;
        let token_key = if req.token_addr.is_contract() {
            req.token_addr.clone()
        } else {
            AssetTokenKey::Native
        };

        let token_coin = if req.token_addr.is_contract() {
            ApiCoinDomain::get_coin_by_token_key_exact(&req.chain_code, req.token_addr.clone())
                .await?
        } else {
            ApiChainTransDomain::main_coin(&req.chain_code).await?
        };

        let balance_str =
            self.query_balance(&req.from_addr, chain_code, token_key, token_coin.decimals).await?;
        let value = conversion::decimal_from_str(&req.value)?;

        tracing::info!(
            trade_no = %req.trade_no,
            balance = %balance_str,
            requested_value = %value,
            token_addr = %req.token_addr,
            source = "shadow_worker_v2",
            "collect_tx:send: 归集金额检查完成"
        );

        Ok(!Self::is_collect_amount_shortage(&balance_str, &req.value)?)
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

        if Self::should_terminal_fail_solana_token_collect_fee_shortage_after_completed_fee_cycle(
            req,
        ) {
            info!(
                trade_no = %req.trade_no,
                service_fee_uploaded_at = ?req.service_fee_uploaded_at,
                tx_fee_res_ack_sent_at = ?req.tx_fee_res_ack_sent_at,
                source = "shadow_worker_v2",
                "Solana token collect fee shortage after completed fee cycle; marking collect as terminal failure"
            );

            let rows_affected = ApiCollectRepo::update_api_collect_status_and_err(
                &self.collect_pool,
                &req.trade_no,
                ApiCollectStatus::InsufficientBalance,
                ErrCode::BalanceInsufficient,
                "solana token collect fee shortage after completed fee cycle",
            )
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;

            info!(
                trade_no = %req.trade_no,
                rows_affected = %rows_affected,
                source = "shadow_worker_v2",
                "Marked Solana token collect as terminal failure after completed fee cycle"
            );
            return Ok(0);
        }

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
        if spend_all_native {
            params.metadata = Some(COLLECT_IGNORE_SENDER_RENT_METADATA.to_string());
        }
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

    async fn estimate_tron_fee_details(
        &self,
        from: &str,
        to: &str,
        value: &str,
        symbol: &str,
        main_symbol: &str,
        token_key: AssetTokenKey,
        decimals: u8,
    ) -> Result<TronFeeDetails, ServiceError> {
        let adapter = ApiChainAdapterFactory::get_transaction_adapter("tron").await?;
        let mut params = ApiBaseTransferReq::new(from, to, value, "tron");
        params.with_token(token_key.to_chain_token_option(), decimals, symbol);
        let fee = adapter.estimate_fee(params, main_symbol).await?;
        let details: TronFeeDetails = wallet_utils::serde_func::serde_from_str(&fee)?;
        Ok(details)
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
        if params.spend_all {
            params.metadata = Some(COLLECT_IGNORE_SENDER_RENT_METADATA.to_string());
        }
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
            if req.service_fee_uploaded_at.is_some() {
                info!(
                    trade_no = %trade_no,
                    error = %err,
                    service_fee_uploaded_at = ?req.service_fee_uploaded_at,
                    tx_fee_res_ack_sent_at = ?req.tx_fee_res_ack_sent_at,
                    source = "shadow_worker_v2",
                    "Detected Solana rent-exempt reserve shortage after completed fee cycle; marking collect as terminal failure"
                );

                self.clear_build_slot_after_claim(trade_no).await?;

                let rows_affected = ApiCollectRepo::update_api_collect_status_and_err(
                    &self.collect_pool,
                    trade_no,
                    ApiCollectStatus::InsufficientBalance,
                    ErrCode::BalanceInsufficient,
                    &format!("{}", err),
                )
                .await
                .map_err(|db_err| {
                    error!(trade_no = %trade_no, error = %db_err, source = "shadow_worker_v2", "Failed to mark collect as terminal failure after completed fee cycle");
                    ServiceError::Database(db_err.into())
                })?;

                info!(
                    trade_no = %trade_no,
                    rows_affected = %rows_affected,
                    source = "shadow_worker_v2",
                    "Marked collect as terminal failure after completed fee cycle"
                );
                self.advancer.try_advance(&req.trade_no).await;
            } else {
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
            }

            return Ok(());
        }

        if Self::is_collect_amount_insufficient_error(&err) {
            info!(
                trade_no = %trade_no,
                error = %err,
                source = "shadow_worker_v2",
                "Detected collect amount insufficient error; marking collect as terminal failure"
            );

            self.clear_build_slot_after_claim(trade_no).await?;

            let rows_affected = ApiCollectRepo::update_api_collect_status_and_err(
                &self.collect_pool,
                trade_no,
                ApiCollectStatus::InsufficientBalance,
                ErrCode::BalanceInsufficient,
                &format!("{}", err),
            )
            .await
            .map_err(|db_err| {
                error!(trade_no = %trade_no, error = %db_err, source = "shadow_worker_v2", "Failed to mark collect as terminal failure due to insufficient balance");
                ServiceError::Database(db_err.into())
            })?;

            info!(
                trade_no = %trade_no,
                rows_affected = %rows_affected,
                source = "shadow_worker_v2",
                "Marked collect as terminal failure due to insufficient balance"
            );
            self.advancer.try_advance(&req.trade_no).await;

            return Ok(());
        }

        if req.raw_tx.is_none() && req.tx_hash.is_none() && Self::is_collect_build_503_error(&err) {
            info!(
                trade_no = %trade_no,
                elapsed_secs = ?Self::build_503_elapsed_secs(&req, Utc::now()),
                retry_window_secs = Self::BUILD_503_RETRY_WINDOW_SECS,
                source = "shadow_worker_v2",
                "Detected BuildTx 503 failure"
            );

            self.clear_build_slot_after_claim(trade_no).await?;

            let now = Utc::now();
            if Self::should_terminal_fail_collect_build_503(&req, &err, now) {
                let rows_affected = ApiCollectRepo::update_api_collect_status_and_err(
                    &self.collect_pool,
                    trade_no,
                    ApiCollectStatus::SendingTxFailed,
                    ErrCode::NetworkException,
                    &format!("{}", err),
                )
                .await
                .map_err(|db_err| {
                    error!(trade_no = %trade_no, error = %db_err, source = "shadow_worker_v2", "Failed to mark collect as terminal failure after repeated BuildTx 503");
                    ServiceError::Database(db_err.into())
                })?;

                info!(
                    trade_no = %trade_no,
                    rows_affected = %rows_affected,
                    elapsed_secs = ?Self::build_503_elapsed_secs(&req, now),
                    retry_window_secs = Self::BUILD_503_RETRY_WINDOW_SECS,
                    source = "shadow_worker_v2",
                    "BuildTx 503 retry window expired; marked collect as terminal failure"
                );

                if rows_affected > 0 {
                    self.advancer.try_advance(&trade_no).await;
                }
            } else {
                let rows_affected = ApiCollectRepo::update_api_collect_post_tx_count(
                    &self.collect_pool,
                    trade_no,
                )
                .await
                .map_err(|db_err| {
                    error!(trade_no = %trade_no, error = %db_err, source = "shadow_worker_v2", "Failed to bump BuildTx 503 retry count");
                    ServiceError::Database(db_err.into())
                })?;

                info!(
                    trade_no = %trade_no,
                    rows_affected = %rows_affected,
                    elapsed_secs = ?Self::build_503_elapsed_secs(&req, now),
                    retry_window_secs = Self::BUILD_503_RETRY_WINDOW_SECS,
                    source = "shadow_worker_v2",
                    "BuildTx 503 retry window active; will retry later"
                );
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
    use crate::{
        error::{service::ServiceError, system::SystemError},
        infrastructure::api_trans::collect::shadow::{ChainIntent, CollectIntent},
    };
    use chrono::Utc;
    use rust_decimal::Decimal;
    use std::{str::FromStr, sync::Arc};
    use tempfile::tempdir;
    use tokio::sync::mpsc;
    use wallet_database::{
        ApiWalletDbPool, SqliteContext,
        entities::{
            api_collect::{ApiCollectEntity, ApiCollectStatus},
            api_resource_delegation::{
                ApiResourceDelegationOperationType, ApiResourceDelegationRecoverStatus,
                ApiResourceDelegationSource, NewApiResourceDelegation,
            },
            api_resource_gate::{
                ApiResourceBlockReason, ApiResourceDependencyType, ApiResourceGateResult,
            },
            api_resource_type::ApiResourceType,
            api_trade_type::ApiTradeType,
            asset_token_key::AssetTokenKey,
        },
        repositories::api_wallet::{
            collect::ApiCollectRepo, resource_delegation::ApiResourceDelegationRepo,
        },
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
            resource_check_at: None,
            resource_gate_released_at: None,
            resource_gate_result: None,
            resource_block_reason: None,
            resource_dependency_trade_no: None,
            resource_dependency_type: None,
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
                intent_tx.clone(),
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
                intent_tx.clone(),
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
                intent_tx.clone(),
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
    fn collect_amount_shortage_detects_balance_below_requested_value() {
        assert!(
            ShadowCollectWorker::is_collect_amount_shortage("1.109998", "1.109999")
                .expect("compare")
        );
        assert!(
            !ShadowCollectWorker::is_collect_amount_shortage("1.109999", "1.109999")
                .expect("compare")
        );
    }

    #[test]
    fn resource_gate_delegate_trade_no_is_deterministic() {
        assert_eq!(
            ShadowCollectWorker::collect_local_delegate_trade_no("C_1"),
            "rsc_local_delegate_C_1"
        );
    }

    #[test]
    fn tron_collect_resource_gate_ignores_bandwidth_shortage() {
        assert!(ShadowCollectWorker::tron_resource_ready(0, 0, 0, 268));
        assert!(!ShadowCollectWorker::tron_resource_ready(0, 1024, 1, 0));
    }

    #[tokio::test]
    async fn release_resource_gate_is_idempotent() {
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
                intent_tx.clone(),
                None,
            )),
        );

        let trade_no = "C_release_once";
        ApiCollectRepo::upsert_api_collect(
            &collect_pool,
            "uid",
            "collect",
            "from",
            "to",
            "1.12",
            "digest",
            "tron",
            None,
            "TRX",
            trade_no,
            2,
            ApiCollectStatus::Init,
            1,
        )
        .await
        .expect("insert collect");

        worker.process_release_resource_gate(trade_no.to_string()).await.expect("first release");
        worker.process_release_resource_gate(trade_no.to_string()).await.expect("second release");

        let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, trade_no)
            .await
            .expect("load collect");
        assert!(persisted.resource_gate_released_at.is_some());
        assert_eq!(persisted.resource_gate_result, Some(ApiResourceGateResult::ResourceReady));
    }

    #[tokio::test]
    async fn local_delegation_block_commit_is_idempotent() {
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
                intent_tx.clone(),
                None,
            )),
        );

        let trade_no = "C_local_block_once";
        ApiCollectRepo::upsert_api_collect(
            &collect_pool,
            "uid",
            "collect",
            "from",
            "to",
            "1.12",
            "digest",
            "tron",
            None,
            "TRX",
            trade_no,
            2,
            ApiCollectStatus::Init,
            1,
        )
        .await
        .expect("insert collect");
        let req = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, trade_no)
            .await
            .expect("load collect");

        worker
            .commit_local_delegation_block(trade_no, &req, "withdraw_owner", 100, 20, 10.0)
            .await
            .expect("first local block commit");
        worker
            .commit_local_delegation_block(trade_no, &req, "withdraw_owner", 100, 20, 10.0)
            .await
            .expect("second local block commit");

        let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, trade_no)
            .await
            .expect("load collect");
        assert_eq!(
            persisted.resource_dependency_trade_no.as_deref(),
            Some("rsc_local_delegate_C_local_block_once")
        );
        assert_eq!(
            persisted.resource_dependency_type,
            Some(ApiResourceDependencyType::LocalDelegate)
        );
        assert_eq!(
            persisted.resource_block_reason,
            Some(ApiResourceBlockReason::NeedLocalDelegate)
        );

        let delegation = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &collect_pool,
            "rsc_local_delegate_C_local_block_once",
        )
        .await
        .expect("load delegation");
        assert_eq!(delegation.source, ApiResourceDelegationSource::Local);
        assert_eq!(delegation.origin_trade_no.as_deref(), Some(trade_no));
        assert_eq!(delegation.owner_address, "withdraw_owner");
        assert_eq!(delegation.receiver_address, "from");
        assert_eq!(delegation.native_amount, "8");
        assert_eq!(delegation.amount, "80");
    }

    #[tokio::test]
    async fn local_delegation_failure_releases_collect_gate() {
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
                intent_tx.clone(),
                None,
            )),
        );

        let trade_no = "C_local_release";
        ApiCollectRepo::upsert_api_collect(
            &collect_pool,
            "uid",
            "collect",
            "from",
            "to",
            "1.12",
            "digest",
            "tron",
            None,
            "TRX",
            trade_no,
            2,
            ApiCollectStatus::Init,
            1,
        )
        .await
        .expect("insert collect");
        ApiCollectRepo::mark_resource_blocked(
            &collect_pool,
            trade_no,
            ApiResourceBlockReason::NeedLocalDelegate,
            Some("rsc_local_delegate_C_local_release"),
            Some(ApiResourceDependencyType::LocalDelegate),
        )
        .await
        .expect("block collect");
        ApiResourceDelegationRepo::upsert(
            &collect_pool,
            NewApiResourceDelegation::local_delegate(
                "uid",
                "rsc_local_delegate_C_local_release",
                trade_no,
                2,
                "withdraw_owner",
                "from",
                "10",
                "10",
            ),
        )
        .await
        .expect("insert local delegation");
        ApiResourceDelegationRepo::mark_failed_if_unfinished(
            &collect_pool,
            "rsc_local_delegate_C_local_release",
            "ERR_6008",
            "local delegate failed",
        )
        .await
        .expect("mark failed");

        worker
            .release_collect_gate_after_local_delegation_failure(
                "rsc_local_delegate_C_local_release",
            )
            .await
            .expect("release collect after local failure");

        let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, trade_no)
            .await
            .expect("load collect");
        assert!(persisted.resource_gate_released_at.is_some());
        assert_eq!(
            persisted.resource_gate_result,
            Some(ApiResourceGateResult::LocalDelegationFailedBypass)
        );
    }

    #[tokio::test]
    async fn platform_delegation_failure_does_not_mark_backend_result_received() {
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

        ApiResourceDelegationRepo::upsert(
            &collect_pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid",
                "rsc_platform_failed",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Delegate,
                "tron",
                "withdraw_owner",
                "from",
                ApiResourceType::Energy,
                "10",
                "10",
            ),
        )
        .await
        .expect("insert platform delegation");
        ApiResourceDelegationRepo::mark_failed_if_unfinished(
            &collect_pool,
            "rsc_platform_failed",
            "ERR_6008",
            "platform delegate failed locally",
        )
        .await
        .expect("mark failed");

        worker
            .release_collect_gate_after_local_delegation_failure("rsc_platform_failed")
            .await
            .expect("ignore platform failure");

        let task = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &collect_pool,
            "rsc_platform_failed",
        )
        .await
        .expect("load platform task");
        assert!(task.result_received_at.is_none());
        assert!(task.result_ack_sent_at.is_none());

        let rows = ApiResourceDelegationRepo::scan_need_result_ack(&collect_pool, 100)
            .await
            .expect("scan result ack");
        assert!(!rows.iter().any(|row| row.resource_trade_no == "rsc_platform_failed"));
    }

    #[tokio::test]
    async fn platform_delegate_retryable_error_releases_build_slot() {
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

        ApiResourceDelegationRepo::upsert(
            &collect_pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid",
                "rsc_platform_delegate_retry",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Delegate,
                "tron",
                "withdraw_owner",
                "from",
                ApiResourceType::Energy,
                "10",
                "10",
            ),
        )
        .await
        .expect("insert platform delegation");
        ApiResourceDelegationRepo::mark_task_ack_sent(&collect_pool, "rsc_platform_delegate_retry")
            .await
            .expect("mark ack");
        ApiResourceDelegationRepo::claim_build_slot(&collect_pool, "rsc_platform_delegate_retry")
            .await
            .expect("claim build slot");

        worker
            .schedule_resource_delegation_rebuild_retry(
                "rsc_platform_delegate_retry",
                &ServiceError::Parameter("retryable test error".to_string()),
            )
            .await
            .expect("schedule retry");

        let task = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &collect_pool,
            "rsc_platform_delegate_retry",
        )
        .await
        .expect("load platform task");
        assert!(task.building_at.is_none());
        assert_eq!(task.tx_hash, None);
        assert_eq!(task.retry_count, 1);
        assert_eq!(task.recover_status, Some(ApiResourceDelegationRecoverStatus::RetryRecover));
        assert!(task.next_retry_at.is_some());
    }

    #[tokio::test]
    async fn local_delegation_success_releases_collect_gate() {
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
                intent_tx.clone(),
                None,
            )),
        );

        let trade_no = "C_local_success";
        ApiCollectRepo::upsert_api_collect(
            &collect_pool,
            "uid",
            "collect",
            "from",
            "to",
            "1.12",
            "digest",
            "tron",
            None,
            "TRX",
            trade_no,
            2,
            ApiCollectStatus::Init,
            1,
        )
        .await
        .expect("insert collect");
        let input = NewApiResourceDelegation::local_delegate(
            "uid",
            "rsc_local_delegate_C_local_success",
            trade_no,
            2,
            "withdraw_owner",
            "from",
            "10",
            "10",
        );
        ApiResourceDelegationRepo::upsert(&collect_pool, input)
            .await
            .expect("insert local delegation");
        let delegation = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &collect_pool,
            "rsc_local_delegate_C_local_success",
        )
        .await
        .expect("load delegation");

        worker
            .release_collect_gate_after_local_delegation_success(&delegation)
            .await
            .expect("release collect after local success");

        let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, trade_no)
            .await
            .expect("load collect");
        assert!(persisted.resource_gate_released_at.is_some());
        assert_eq!(
            persisted.resource_gate_result,
            Some(ApiResourceGateResult::LocalDelegationSuccess)
        );
    }

    #[test]
    fn resource_shortfall_saturates_at_zero() {
        assert_eq!(ShadowCollectWorker::resource_shortfall(100, 40), 60);
        assert_eq!(ShadowCollectWorker::resource_shortfall(100, 120), 0);
        assert_eq!(ShadowCollectWorker::resource_shortfall(100, -1), 100);
    }

    #[test]
    fn sol_native_collect_keeps_sender_rent_bypass_scope() {
        assert!(ShadowCollectWorker::should_spend_all_native_collect(
            "sol",
            &AssetTokenKey::Native
        ));
        assert!(!ShadowCollectWorker::should_spend_all_native_collect(
            "sol",
            &AssetTokenKey::Contract("token".to_string())
        ));
    }

    #[test]
    fn sol_token_collect_fee_need_includes_sender_rent_reserve() {
        let fee = Decimal::from_str("0.000015").expect("fee");
        let rent_reserve = ShadowCollectWorker::sol_token_collect_sender_rent_reserve(
            "sol",
            &AssetTokenKey::Contract("token".to_string()),
        )
        .expect("rent reserve");
        let need = fee + rent_reserve;

        assert!(rent_reserve > Decimal::ZERO);
        assert_eq!(need - fee, rent_reserve);
    }

    #[test]
    fn sol_token_collect_exact_log_balance_is_still_short_after_fee_plus_rent() {
        let balance = Decimal::from_str("0.00094588").expect("balance");
        let fee = Decimal::from_str("0.000015").expect("fee");
        let rent_reserve = ShadowCollectWorker::sol_token_collect_sender_rent_reserve(
            "sol",
            &AssetTokenKey::Contract("token".to_string()),
        )
        .expect("rent reserve");
        let need = fee + rent_reserve;

        assert_eq!(need, Decimal::from_str("0.00100588").expect("need"));
        assert!(balance < need);
    }

    #[test]
    fn sol_token_collect_rent_shortage_reopens_fee_cycle() {
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

        assert!(ShadowCollectWorker::should_reopen_fee_cycle_for_solana_token_collect(&req, &err));
    }

    #[test]
    fn native_sol_collect_does_not_reopen_token_rent_fee_cycle() {
        use crate::error::{
            business::{
                BusinessError,
                chain::{ChainError, InsufficientBalanceDetail},
            },
            service::ServiceError,
        };

        let mut req = base_collect();
        req.token_addr = AssetTokenKey::Native;
        let err = ServiceError::Business(BusinessError::Chain(ChainError::InsufficientBalance(
            InsufficientBalanceDetail::new()
                .reason("sender balance must keep rent-exempt reserve after transfer"),
        )));

        assert!(!ShadowCollectWorker::should_reopen_fee_cycle_for_solana_token_collect(&req, &err));
    }

    #[test]
    fn sol_token_collect_rent_shortage_after_fee_cycle_terminates() {
        use crate::error::{
            business::{
                BusinessError,
                chain::{ChainError, InsufficientBalanceDetail},
            },
            service::ServiceError,
        };

        let mut req = base_collect();
        req.service_fee_uploaded_at = Some(Utc::now());
        let err = ServiceError::Business(BusinessError::Chain(ChainError::InsufficientBalance(
            InsufficientBalanceDetail::new()
                .reason("sender balance must keep rent-exempt reserve after transfer"),
        )));

        assert!(ShadowCollectWorker::should_terminal_fail_solana_token_collect_rent_shortage(
            &req, &err
        ));
    }

    #[test]
    fn sol_token_collect_fee_shortage_after_fee_cycle_terminates() {
        let mut req = base_collect();
        req.service_fee_uploaded_at = Some(Utc::now());

        assert!(
            ShadowCollectWorker::should_terminal_fail_solana_token_collect_fee_shortage_after_completed_fee_cycle(&req)
        );
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

    #[test]
    fn sol_chain_tracks_broadcast_uncertain_state() {
        assert!(ShadowCollectWorker::tracks_broadcast_uncertain_state("sol"));
        assert!(ShadowCollectWorker::tracks_broadcast_uncertain_state("eth"));
        assert!(ShadowCollectWorker::tracks_broadcast_uncertain_state("bnb"));
        assert!(!ShadowCollectWorker::tracks_broadcast_uncertain_state("tron"));
    }

    #[test]
    fn broadcast_uncertain_timeout_helper_trips_after_five_minutes() {
        let mut req = base_collect();
        req.broadcast_uncertain_since_at = Some(Utc::now() - chrono::TimeDelta::minutes(5));

        assert!(ShadowCollectWorker::should_auto_fail_broadcast_uncertain(&req, Utc::now()));
    }

    #[test]
    fn build_503_error_detector_matches_node_503_message() {
        let err = crate::error::service::ServiceError::System(
            crate::error::system::SystemError::Internal(
                "Node response error: code=503, rpc=https://api.nileex.io/wallet/getaccount"
                    .to_string(),
            ),
        );

        assert!(ShadowCollectWorker::is_collect_build_503_error(&err));
    }

    #[test]
    fn build_503_terminal_failure_trips_after_time_window() {
        let mut req = base_collect();
        req.updated_at = Some(Utc::now() - chrono::TimeDelta::minutes(4));
        let err = crate::error::service::ServiceError::System(
            crate::error::system::SystemError::Internal(
                "Node response error: code=503, rpc=https://api.nileex.io/wallet/getaccount"
                    .to_string(),
            ),
        );

        assert!(ShadowCollectWorker::should_terminal_fail_collect_build_503(
            &req,
            &err,
            Utc::now()
        ));
    }
}
