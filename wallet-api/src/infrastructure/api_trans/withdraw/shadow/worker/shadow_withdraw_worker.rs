// withdraw/shadow/worker/shadow_withdraw_worker.rs
#![allow(deprecated)]

use std::sync::Arc;

use chrono::Utc;
use tracing::{error, info, warn};
use wallet_chain_interact::{
    BillResourceConsume,
    tron::operations::{RawTransactionParams, TronTxOperation},
};
use wallet_database::{
    ApiResourceDbPool, ApiTransactionDbPool, ApiWalletDbPool,
    entities::{
        api_resource_delegation::{
            ApiResourceDelegationEntity, ApiResourceDelegationOperationType,
            ApiResourceDelegationRecoverStatus,
        },
        api_resource_gate::{
            ApiResourceBlockReason, ApiResourceDependencyType, ApiResourceGateResult,
        },
        api_resource_type::ApiResourceType,
        api_trade_type::ApiTradeType,
        api_withdraw::{
            ApiWithdrawEntity, ErrCode, PendingWithdrawResourceBudget, WithdrawFailureStage,
        },
    },
    repositories::api_wallet::{
        resource_delegation::ApiResourceDelegationRepo, wallet::ApiWalletRepo,
        withdraw::ApiWithdrawRepo,
    },
};
use wallet_transport_backend::request::api_wallet::{
    resource_delegation::{ResourceApplyReq, ResourceType},
    transaction::TransType,
};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::RetryableError as _;

use crate::{
    domain::{
        api_wallet::{
            adapter::tx::RawTx,
            adapter_factory::ApiChainAdapterFactory,
            coin::ApiCoinDomain,
            trans::{ApiTransDomain, broadcast_mechanism, shared_mechanism},
            wallet::ApiWalletDomain,
        },
        chain::adapter::ChainAdapterFactory,
    },
    error::{
        business::{
            BusinessError,
            api_wallet::{ApiWalletError, trans::TransError},
            chain::ChainError,
        },
        service::ServiceError,
        system::SystemError,
    },
    infrastructure::{
        api_trans::{
            nonce_recovery_mechanism,
            resource_amount::{
                energy_shortfall_to_apply_amounts, parse_resource_delegation_native_trx_units,
            },
            resource_apply_outcome::PlatformApplyOutcome,
            resource_authorization::{
                ResourceDelegationSigner, new_tron_delegate_args, new_tron_undelegate_args,
                resolve_resource_delegation_signer,
            },
            resource_delegation::{
                execute_resource_delegation, resource_delegation_failure_fact,
                resource_delegation_retry_wait_secs,
            },
            resource_reservation::{
                ResourceBudget, ResourceReservationDecision, WithdrawResourceReservationManager,
            },
            resource_rpc_auth,
            withdraw::shadow::ShadowScanner,
        },
        nonce::nonce_engine::{ReconcileReason, get_nonce_engine_with_ctx},
    },
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
    response_vo::{TronFeeDetails, api_wallet::transaction::ApiWalletResourceConsume},
};
use wallet_database::entities::asset_token_key::AssetTokenKey;
use wallet_utils::{RetryableError as _, conversion, unit};

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
    ctx: &'static crate::context::Context,
    /// ShadowScanner 引用，用于直接调用 try_advance
    scanner: Arc<ShadowScanner>,
    resource_reservations: WithdrawResourceReservationManager,
}

impl ShadowWithdrawWorker {
    const TRON_EXPIRED_RAW_MISSING_CONFIRMED_SETTLE_BUFFER_MS: i64 = 30_000;
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

    fn evm_uncertain_backoff_secs(retry_count: u32) -> i64 {
        shared_mechanism::broadcast_uncertain_backoff_secs(
            retry_count,
            Self::evm_uncertain_backoff_mid_secs(),
            Self::evm_uncertain_backoff_max_secs(),
        )
    }

    fn evm_uncertain_elapsed_secs(
        req: &ApiWithdrawEntity,
        now: chrono::DateTime<Utc>,
    ) -> Option<i64> {
        shared_mechanism::broadcast_uncertain_elapsed_secs(req.broadcast_uncertain_since_at, now)
    }

    fn should_mark_evm_uncertain_for_manual_review(
        req: &ApiWithdrawEntity,
        now: chrono::DateTime<Utc>,
    ) -> bool {
        shared_mechanism::is_broadcast_uncertain_timed_out(
            req.broadcast_uncertain_since_at,
            Self::EVM_UNCERTAIN_MANUAL_REVIEW_TIMEOUT_SECS,
            now,
        )
    }

    fn should_throttle_evm_uncertain_recover(
        req: &ApiWithdrawEntity,
        now: chrono::DateTime<Utc>,
    ) -> bool {
        let Some(since) = req.broadcast_uncertain_since_at else {
            return false;
        };
        let elapsed = now.signed_duration_since(since).num_seconds();
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
        shared_mechanism::should_throttle_broadcast_uncertain_after_last_check(
            req.broadcast_uncertain_last_checked_at,
            wait_secs,
            now,
        )
    }

    fn should_fail_expired_tron_missing_confirmed(req: &ApiWithdrawEntity) -> bool {
        shared_mechanism::should_fail_expired_tron_missing_confirmed(
            &req.chain_code,
            req.raw_tx.as_deref(),
            Utc::now().timestamp_millis(),
            Self::TRON_EXPIRED_RAW_MISSING_CONFIRMED_SETTLE_BUFFER_MS,
        )
    }

    fn build_failure_err_code(err: &ServiceError) -> Option<ErrCode> {
        match err {
            ServiceError::Business(BusinessError::Chain(ChainError::InsufficientBalance(_))) => {
                Some(ErrCode::BalanceInsufficient)
            }
            ServiceError::Business(BusinessError::Chain(ChainError::InsufficientFeeBalance)) => {
                Some(ErrCode::FeeInsufficient)
            }
            _ => None,
        }
    }

    fn is_withdraw_amount_shortage(balance: &str, value: &str) -> Result<bool, ServiceError> {
        let balance = conversion::decimal_from_str(balance)?;
        let value = conversion::decimal_from_str(value)?;
        Ok(balance < value)
    }

    fn can_write_fee_estimate_snapshot(req: &ApiWithdrawEntity) -> bool {
        req.trade_type == ApiTradeType::Withdraw
            && req.fee_estimated_at.is_none()
            && req.raw_tx.is_none()
            && req.transaction_time.is_none()
            && req.finished_at.is_none()
            && req.err_code.is_none()
    }

    fn tron_fee_estimate_resource_json(
        energy: u64,
        bandwidth: u64,
    ) -> Result<String, ServiceError> {
        ApiWalletResourceConsume::new(bandwidth, energy).to_json_string()
    }

    fn tron_actual_resource_json(consumer: BillResourceConsume) -> Result<String, ServiceError> {
        ApiWalletResourceConsume::from_bill_resource_consume(consumer).to_json_string()
    }

    fn tron_resource_ready(
        available_energy: i64,
        available_bandwidth: i64,
        required_energy: u64,
        required_bandwidth: u64,
    ) -> bool {
        available_energy >= required_energy as i64
            && available_bandwidth >= required_bandwidth as i64
    }

    fn effective_resource_after_pending_withdraws(
        available_energy: i64,
        available_bandwidth: i64,
        pending_budget: PendingWithdrawResourceBudget,
    ) -> ResourceBudget {
        let effective_energy =
            available_energy.saturating_sub(pending_budget.energy.min(i64::MAX as u64) as i64);
        let effective_bandwidth = available_bandwidth
            .saturating_sub(pending_budget.bandwidth.min(i64::MAX as u64) as i64);
        ResourceBudget::new(effective_energy.max(0) as u64, effective_bandwidth.max(0) as u64)
    }

    fn resource_shortfall(required: u64, available: i64) -> u64 {
        required.saturating_sub(available.max(0) as u64)
    }

    fn energy_delegation_shortfall_amount(
        required_energy: u64,
        available_energy: i64,
    ) -> Option<String> {
        let shortfall = Self::resource_shortfall(required_energy, available_energy);
        if shortfall == 0 { None } else { Some(shortfall.to_string()) }
    }

    fn platform_apply_outcome(
        is_success: bool,
        dl_trade_no: Option<String>,
    ) -> PlatformApplyOutcome {
        PlatformApplyOutcome::from_backend_response(is_success, dl_trade_no)
    }

    pub fn new(ctx: &'static crate::context::Context, scanner: Arc<ShadowScanner>) -> Self {
        Self { ctx, scanner, resource_reservations: WithdrawResourceReservationManager::default() }
    }
    /// 处理命令
    pub async fn handle(&self, command: super::ShadowWithdrawCommand) -> Result<(), ServiceError> {
        match command {
            super::ShadowWithdrawCommand::EstimateFee(trade_no) => {
                self.process_fee_estimate(trade_no).await
            }
            super::ShadowWithdrawCommand::EvalResourceGate(trade_no) => {
                self.process_eval_resource_gate(trade_no).await
            }
            super::ShadowWithdrawCommand::BuildTx(trade_no) => {
                self.process_build_tx(trade_no).await
            }
            super::ShadowWithdrawCommand::Broadcast(trade_no) => {
                self.process_broadcast(trade_no).await
            }
            super::ShadowWithdrawCommand::Recover(trade_no) => self.process_recover(trade_no).await,
            super::ShadowWithdrawCommand::ExecuteResourceDelegation(trade_no) => {
                self.process_resource_delegation_execute(trade_no).await
            }
        }
    }

    async fn process_eval_resource_gate(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Processing EvalResourceGate command");
        if let Err(err) = crate::infrastructure::chain_rpc_guard::run_guarded_with_ctx(
            self.ctx,
            "tron",
            &trade_no,
            "withdraw_eval_resource_gate",
            "shadow_withdraw_worker",
            resource_rpc_auth::run_with_rpc_auth_retry(
                "tron",
                "withdraw_eval_resource_gate",
                &trade_no,
                || self.process_resource_gate_inner(&trade_no),
            ),
        )
        .await
        {
            if let Some(err_code) = Self::build_failure_err_code(&err) {
                self.mark_prebuild_terminal_failure(&trade_no, err_code, &err).await?;
                return Ok(());
            }
            return Err(err);
        }
        Ok(())
    }

    async fn mark_prebuild_terminal_failure(
        &self,
        trade_no: &str,
        err_code: ErrCode,
        err: &ServiceError,
    ) -> Result<(), ServiceError> {
        let error_msg = err.to_string();
        let rows_affected = ApiWithdrawRepo::mark_build_failed_if_unfinished(
            &self.ctx.api_transaction_pool()?,
            trade_no,
            err_code,
            &error_msg,
        )
        .await
        .map_err(|db_err: wallet_database::Error| {
            error!(trade_no = %trade_no, error = %db_err, source = "shadow_withdraw_worker", "Failed to mark pre-BuildTx terminal failure");
            ServiceError::Database(db_err.into())
        })?;

        info!(
            trade_no = %trade_no,
            rows_affected = %rows_affected,
            err_code = %err_code,
            source = "shadow_withdraw_worker",
            "Marked pre-BuildTx terminal failure"
        );

        if rows_affected > 0 {
            self.scanner.try_advance(trade_no).await;
        }

        Ok(())
    }

    async fn process_fee_estimate(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Processing EstimateFee command");
        let req = self.get_withdraw_entity(&trade_no).await?;
        if !Self::can_write_fee_estimate_snapshot(&req) {
            return Ok(());
        }

        let Some((estimated_transaction_fee, estimated_resource_consume)) =
            crate::infrastructure::chain_rpc_guard::run_guarded_with_ctx(
                self.ctx,
                &req.chain_code,
                &trade_no,
                "withdraw_estimate_fee",
                "shadow_withdraw_worker",
                resource_rpc_auth::run_with_rpc_auth_retry(
                    &req.chain_code,
                    "withdraw_estimate_fee",
                    &trade_no,
                    || self.estimate_fee_snapshot_for_withdraw(&req),
                ),
            )
            .await?
        else {
            return Ok(());
        };

        let rows = ApiWithdrawRepo::update_fee_estimate(
            &self.ctx.api_transaction_pool()?,
            &trade_no,
            &estimated_transaction_fee,
            &estimated_resource_consume,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        info!(
            trade_no = %trade_no,
            affected = rows,
            estimated_transaction_fee = %estimated_transaction_fee,
            estimated_resource_consume = %estimated_resource_consume,
            source = "shadow_withdraw_worker",
            "Withdraw fee estimate snapshot write attempted"
        );
        Ok(())
    }

    async fn handle_resource_delegation_terminal_failure_if_needed(
        &self,
        resource_trade_no: &str,
        result: Result<(), ServiceError>,
    ) -> Result<(), ServiceError> {
        let Err(err) = result else {
            return Ok(());
        };

        if err.is_transient_db_pool_timeout() {
            self.schedule_resource_delegation_rebuild_retry(resource_trade_no, &err).await?;
            warn!(
                resource_trade_no = %resource_trade_no,
                error = %err,
                source = "shadow_withdraw_worker",
                "Resource delegation hit local DB pool timeout, scheduled retry instead of reporting failure"
            );
            return Ok(());
        }

        match err.retry_policy() {
            wallet_utils::RetryPolicy::Never => {
                self.mark_resource_delegation_failed(resource_trade_no, &err).await?;
                Ok(())
            }
            wallet_utils::RetryPolicy::Delay => {
                self.schedule_resource_delegation_rebuild_retry(resource_trade_no, &err).await?;
                info!(
                    resource_trade_no = %resource_trade_no,
                    error = %err,
                    source = "shadow_withdraw_worker",
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
        Self::schedule_resource_delegation_rebuild_retry_in_pool(
            &self.ctx.api_resource_pool()?,
            resource_trade_no,
            err,
        )
        .await
    }

    async fn schedule_resource_delegation_rebuild_retry_in_pool(
        pool: &ApiResourceDbPool,
        resource_trade_no: &str,
        err: &ServiceError,
    ) -> Result<(), ServiceError> {
        let task = ApiResourceDelegationRepo::get_by_resource_trade_no(pool, resource_trade_no)
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
            pool,
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
            source = "shadow_withdraw_worker",
            "Withdraw resource delegation reset for retry"
        );

        Ok(())
    }

    async fn process_resource_delegation_execute(
        &self,
        resource_trade_no: String,
    ) -> Result<(), ServiceError> {
        let affected = ApiResourceDelegationRepo::claim_build_slot(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            return Ok(());
        }

        let delegation = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        if delegation.tx_hash.is_some() {
            return Ok(());
        }

        let tx_hash = self.execute_tron_resource_delegation(&delegation).await?;
        ApiResourceDelegationRepo::mark_broadcast_success(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
            &tx_hash,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        Ok(())
    }

    async fn process_resource_gate_inner(&self, trade_no: &str) -> Result<(), ServiceError> {
        let req = self.get_withdraw_entity(trade_no).await?;

        if !shared_mechanism::is_tron_chain(&req.chain_code) {
            return Ok(());
        }
        if req.resource_gate_released_at.is_some()
            || req.raw_tx.is_some()
            || req.transaction_time.is_some()
            || req.finished_at.is_some()
            || req.err_code.is_some()
        {
            return Ok(());
        }
        if Self::is_waiting_platform_delegate_result(&req) {
            info!(
                trade_no = %trade_no,
                dependency_trade_no = ?req.resource_dependency_trade_no,
                source = "shadow_withdraw_worker",
                "TRON withdraw resource gate already blocked on platform delegation result"
            );
            return Ok(());
        }

        let fee_details = self.estimate_tron_fee_details_for_withdraw(&req).await?;
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter_with_ctx(self.ctx, &req.chain_code)
                .await?;
        let resource = adapter.account_resource(&req.from_addr).await?;
        let available_energy = resource.available_energy();
        let available_bandwidth = resource.available_bandwidth();
        let pending_budget = ApiWithdrawRepo::pending_resource_budget_by_owner(
            &self.ctx.api_transaction_pool()?,
            &req.chain_code,
            &req.from_addr,
            trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        let required = ResourceBudget::new(fee_details.energy, fee_details.bandwidth);
        let available = Self::effective_resource_after_pending_withdraws(
            available_energy,
            available_bandwidth,
            pending_budget,
        );

        let decision = self.resource_reservations.reserve_if_available(
            &req.from_addr,
            trade_no.to_string(),
            required,
            available,
        );

        if let ResourceReservationDecision::Reserved { effective_available, reserved } = decision {
            self.log_resource_gate_decision(
                trade_no,
                &req,
                "resource_ready",
                &fee_details,
                available_energy,
                available_bandwidth,
                pending_budget,
                reserved,
                effective_available,
            )
            .await?;
            let rows = ApiWithdrawRepo::mark_resource_released(
                &self.ctx.api_transaction_pool()?,
                trade_no,
                ApiResourceGateResult::ResourceReady,
            )
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
            if rows > 0 {
                self.scanner.try_advance(trade_no).await;
            }
            return Ok(());
        }

        let ResourceReservationDecision::NeedMore(shortfall) = decision else {
            unreachable!("reserved decision returned early")
        };
        self.log_resource_gate_decision(
            trade_no,
            &req,
            "need_platform_delegate",
            &fee_details,
            available_energy,
            available_bandwidth,
            pending_budget,
            shortfall.reserved,
            shortfall.effective_available,
        )
        .await?;

        let result = self
            .commit_platform_delegation_block(
                trade_no,
                &req,
                fee_details.energy,
                fee_details.bandwidth,
                shortfall.effective_available.energy as i64,
                shortfall.effective_available.bandwidth as i64,
            )
            .await;
        if result.is_err() {
            self.resource_reservations.release(trade_no);
        }
        result
    }

    fn is_waiting_platform_delegate_result(req: &ApiWithdrawEntity) -> bool {
        req.resource_block_reason == Some(ApiResourceBlockReason::NeedPlatformDelegate)
            && req.resource_dependency_type == Some(ApiResourceDependencyType::PlatformDelegate)
            && req.resource_dependency_trade_no.is_some()
    }

    async fn log_resource_gate_decision(
        &self,
        trade_no: &str,
        req: &ApiWithdrawEntity,
        decision: &'static str,
        fee_details: &TronFeeDetails,
        available_energy: i64,
        available_bandwidth: i64,
        pending_budget: PendingWithdrawResourceBudget,
        reserved: ResourceBudget,
        effective_available: ResourceBudget,
    ) -> Result<(), ServiceError> {
        info!(
            trade_no = %trade_no,
            from_addr = %req.from_addr,
            decision,
            required_energy = %fee_details.energy,
            required_bandwidth = %fee_details.bandwidth,
            chain_available_energy = %available_energy,
            chain_available_bandwidth = %available_bandwidth,
            pending_db_energy = %pending_budget.energy,
            pending_db_bandwidth = %pending_budget.bandwidth,
            reserved_memory_energy = %reserved.energy,
            reserved_memory_bandwidth = %reserved.bandwidth,
            effective_energy = %effective_available.energy,
            effective_bandwidth = %effective_available.bandwidth,
            source = "shadow_withdraw_worker",
            "TRON withdraw resource gate budget decision"
        );

        if pending_budget.energy == 0 && pending_budget.bandwidth == 0 {
            return Ok(());
        }

        let pending_orders = ApiWithdrawRepo::pending_resource_orders_by_owner(
            &self.ctx.api_transaction_pool()?,
            &req.chain_code,
            &req.from_addr,
            trade_no,
            10,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        for pending in pending_orders {
            info!(
                trade_no = %trade_no,
                from_addr = %req.from_addr,
                pending_trade_no = %pending.trade_no,
                pending_estimated_resource_consume = ?pending.estimated_resource_consume,
                pending_resource_gate_result = ?pending.resource_gate_result,
                pending_resource_block_reason = ?pending.resource_block_reason,
                pending_resource_dependency_trade_no = ?pending.resource_dependency_trade_no,
                pending_raw_tx_exists = pending.raw_tx.is_some(),
                pending_last_broadcast_at = ?pending.last_broadcast_at,
                pending_tx_hash = ?pending.tx_hash,
                pending_created_at = %pending.created_at.to_rfc3339(),
                source = "shadow_withdraw_worker",
                "TRON withdraw resource gate pending DB budget owner"
            );
        }

        Ok(())
    }

    async fn commit_platform_delegation_block(
        &self,
        origin_trade_no: &str,
        req: &ApiWithdrawEntity,
        required_energy: u64,
        required_bandwidth: u64,
        available_energy: i64,
        available_bandwidth: i64,
    ) -> Result<(), ServiceError> {
        let Some(amount) =
            Self::energy_delegation_shortfall_amount(required_energy, available_energy)
        else {
            let rows = ApiWithdrawRepo::mark_resource_released(
                &self.ctx.api_transaction_pool()?,
                origin_trade_no,
                ApiResourceGateResult::FallbackAllowed,
            )
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
            if rows > 0 {
                self.scanner.try_advance(origin_trade_no).await;
            }
            info!(
                origin_trade_no = %origin_trade_no,
                required_energy = %required_energy,
                available_energy = %available_energy,
                required_bandwidth = %required_bandwidth,
                available_bandwidth = %available_bandwidth,
                source = "shadow_withdraw_worker",
                "TRON withdraw resource gate skipped ENERGY delegation because local ENERGY is enough"
            );
            return Ok(());
        };

        info!(
            origin_trade_no = %origin_trade_no,
            required_energy = %required_energy,
            available_energy = %available_energy,
            energy_shortfall = %amount,
            required_bandwidth = %required_bandwidth,
            available_bandwidth = %available_bandwidth,
            source = "shadow_withdraw_worker",
            "TRON withdraw resource gate requires platform ENERGY delegation"
        );
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
            PlatformApplyOutcome::AcceptedWithResourceTradeNo(resource_trade_no) => {
                resource_trade_no
            }
            // Some backend versions accept the platform delegation request but
            // push the resource result with the original withdraw trade number
            // instead of returning a dedicated WD... delegation trade number.
            // Keep the gate blocked on the original order until TX_RSC_RES is
            // received and ACKed; otherwise BuildTx can outrun the delegation.
            PlatformApplyOutcome::AcceptedWithOriginalTradeNo => origin_trade_no.to_string(),
            PlatformApplyOutcome::Rejected => {
                let rows = ApiWithdrawRepo::mark_resource_released(
                    &self.ctx.api_transaction_pool()?,
                    origin_trade_no,
                    ApiResourceGateResult::FallbackAllowed,
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
                if rows > 0 {
                    self.scanner.try_advance(origin_trade_no).await;
                }
                return Ok(());
            }
        };
        ApiWithdrawRepo::mark_resource_blocked(
            &self.ctx.api_transaction_pool()?,
            origin_trade_no,
            ApiResourceBlockReason::NeedPlatformDelegate,
            Some(resource_trade_no.as_str()),
            Some(ApiResourceDependencyType::PlatformDelegate),
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        info!(
            origin_trade_no = %origin_trade_no,
            required_energy = %required_energy,
            available_energy = %available_energy,
            required_bandwidth = %required_bandwidth,
            available_bandwidth = %available_bandwidth,
            resource_trade_no = ?resource_trade_no,
            source = "shadow_withdraw_worker",
            "TRON withdraw resource gate blocked"
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
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter_with_ctx(self.ctx, chain_code).await?;
        let resource = adapter.account_resource(receiver_address).await?;
        let amounts = energy_shortfall_to_apply_amounts(amount, resource.energy_price())?;
        let (app_id, org_id) = self.resolve_resource_apply_identity(uid).await?;

        info!(
            origin_trade_no = %origin_trade_no,
            receiver_address = %receiver_address,
            energy_shortfall = %amounts.resource_amount,
            energy_price = %resource.energy_price(),
            native_token_amount = %amounts.native_token_amount,
            source = "shadow_withdraw_worker",
            "TRON withdraw platform resource delegation amount calculated"
        );

        let req = ResourceApplyReq::new(
            origin_trade_no,
            &app_id,
            &org_id,
            Some(chain_code),
            amounts.native_token_amount,
            Some(amounts.resource_amount),
            ResourceType::Energy,
            receiver_address,
            TransType::Wd,
        );

        let backend_api = self.ctx.get_global_backend_api();
        let resp = backend_api.apply_resource_delegation(&req).await?;
        let is_success = resp.is_success();
        let outcome = Self::platform_apply_outcome(is_success, resp.dl_trade_no);

        match &outcome {
            PlatformApplyOutcome::AcceptedWithResourceTradeNo(resource_trade_no) => {
                info!(
                    origin_trade_no = %origin_trade_no,
                    resource_trade_no = %resource_trade_no,
                    source = "shadow_withdraw_worker",
                    "Platform resource delegation apply succeeded"
                );
            }
            PlatformApplyOutcome::AcceptedWithOriginalTradeNo => {
                warn!(
                    origin_trade_no = %origin_trade_no,
                    source = "shadow_withdraw_worker",
                    "Platform resource delegation apply succeeded without delegation trade no; waiting original-order resource result"
                );
            }
            PlatformApplyOutcome::Rejected => {
                warn!(
                    origin_trade_no = %origin_trade_no,
                    source = "shadow_withdraw_worker",
                    "Platform resource delegation apply rejected, will proceed with main currency check"
                );
            }
        }
        Ok(outcome)
    }

    async fn resolve_resource_apply_identity(
        &self,
        uid: &str,
    ) -> Result<(String, String), ServiceError> {
        let wallet = ApiWalletRepo::find_by_uid(&self.ctx.api_wallet_pool()?, uid)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        let app_id = wallet.as_ref().and_then(|w| w.app_id.as_deref()).unwrap_or(uid);
        let org_id = wallet.as_ref().and_then(|w| w.merchant_id.as_deref()).unwrap_or(uid);
        Ok((app_id.to_string(), org_id.to_string()))
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

            if shared_mechanism::tracks_broadcast_uncertain_state(&fresh_req.chain_code)
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
                    "Skip Recover: raw_tx exists but not in uncertain state; broadcast should proceed"
                );
                return Ok(());
            }

            fresh_req
        };

        if shared_mechanism::is_evm_chain(&req.chain_code) {
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
                    observability_event = "shadow_recover_skipped",
                    reason = "evm_uncertain_backoff",
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

        // 执行恢复交易。raw wrapper 只处理外层 RPC guard/timeout，保留 recover 内层 Result。
        let recover_result =
            match crate::infrastructure::chain_rpc_guard::run_guarded_raw_with_ctx_and_log(
                self.ctx,
                &req.chain_code,
                trade_no,
                "recover",
                "shadow_withdraw_worker",
                crate::infrastructure::chain_rpc_guard::ChainRpcFailureLog::new()
                    .trade_no(trade_no)
                    .tx_hash(req.tx_hash.as_deref().unwrap_or_default())
                    .chain_code(&req.chain_code)
                    .source("shadow_withdraw_worker")
                    .op("recover"),
                self.recover_tx(&req),
            )
            .await?
            {
                Some(result) => result,
                None => return Ok(()),
            };
        match recover_result {
            Ok(Some(tx_resp)) => {
                info!(trade_no = %trade_no, tx_hash = %tx_resp.tx_hash, source = "shadow_withdraw_worker", "Transaction recover successful");

                // ====== phase 3: 提交不可逆事实 ======
                {
                    // 🔒 必须重新读取，确保基于最新状态做决策
                    let fresh_req = self.get_withdraw_entity(trade_no).await?;

                    let existing_tx_hash =
                        shared_mechanism::normalized_tx_hash(fresh_req.tx_hash.as_deref());

                    if fresh_req.transaction_time.is_some() {
                        let should_nudge = true;

                        if existing_tx_hash.is_none() {
                            let rows_affected = ApiWithdrawRepo::backfill_tx_hash_if_missing(
                                &self.ctx.api_transaction_pool()?,
                                &fresh_req.trade_no,
                                &tx_resp.tx_hash,
                                "shadow_withdraw_worker",
                            )
                            .await
                            .map_err(|e| ServiceError::Database(e.into()))?;

                            info!(
                                trade_no = %fresh_req.trade_no,
                                tx_hash = %tx_resp.tx_hash,
                                rows_affected = %rows_affected,
                                source = "shadow_withdraw_worker",
                                "Recovered tx hash backfilled after concurrent missing-hash read"
                            );
                        }

                        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "tx_hash empty or transaction_time exists, skipping Recover fact commit");
                        if should_nudge {
                            info!(
                                trade_no = %trade_no,
                                source = "shadow_withdraw_worker",
                                "Recover facts already present; nudging scanner for downstream stages"
                            );
                            self.scanner.try_advance(&fresh_req.trade_no).await;
                        }
                        return Ok(());
                    }

                    // 🔒 事实保护：检查 tx_hash 一致性，防止事实被覆盖
                    shared_mechanism::validate_recovered_tx_hash(
                        &fresh_req.trade_no,
                        existing_tx_hash,
                        &tx_resp.tx_hash,
                        "shadow_withdraw_worker",
                    )?;

                    let resource_consume = if let Some(consumer) = tx_resp.consumer {
                        Self::tron_actual_resource_json(consumer)?
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
                            &self.ctx.api_transaction_pool()?,
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
                    if shared_mechanism::should_invalidate_expired_tron_raw_for_recover(
                        &req.chain_code,
                        raw_tx_json,
                        req.last_broadcast_at.is_some(),
                        req.tx_hash.as_deref().is_some_and(|tx_hash| !tx_hash.trim().is_empty()),
                        Utc::now().timestamp_millis(),
                    ) {
                        warn!(
                            trade_no = %req.trade_no,
                            tx_hash = %req.tx_hash.as_deref().unwrap_or_default(),
                            source = "shadow_withdraw_worker",
                            "Detected expired tron raw_tx during recover; invalidating stale tx facts"
                        );
                        let rows = ApiWithdrawRepo::invalidate_raw_tx(
                            &self.ctx.api_transaction_pool()?,
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

                if !shared_mechanism::tracks_broadcast_uncertain_state(&req.chain_code) {
                    // 未纳入 broadcast-uncertain 轨道的链保持原行为：立即尝试推进一次
                    self.scanner.try_advance(trade_no).await;
                    return Ok(());
                }

                let now = Utc::now();
                let rows_affected = ApiWithdrawRepo::mark_broadcast_uncertain_attempt(
                    &self.ctx.api_transaction_pool()?,
                    trade_no,
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
                let refreshed = self.get_withdraw_entity(trade_no).await?;
                info!(
                    observability_event = "shadow_broadcast_uncertain_recorded",
                    reason = "recover_result_uncertain",
                    trade_no = %refreshed.trade_no,
                    tx_hash = %refreshed.tx_hash.as_deref().unwrap_or_default(),
                    nonce = refreshed.nonce,
                    rows_affected = %rows_affected,
                    retry_count = refreshed.broadcast_uncertain_retry_count,
                    uncertain_since_at = ?refreshed.broadcast_uncertain_since_at,
                    reconciled_at = ?refreshed.broadcast_uncertain_reconciled_at,
                    rebroadcast_count = refreshed.broadcast_uncertain_rebroadcast_count,
                    source = "shadow_withdraw_worker",
                    "Recover uncertain state recorded"
                );

                if !shared_mechanism::is_evm_chain(&refreshed.chain_code) {
                    return Ok(());
                }

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

                    let nonce_engine = get_nonce_engine_with_ctx(self.ctx)?;
                    nonce_engine.trigger_reconcile_with_reason(
                        &refreshed.from_addr,
                        &refreshed.chain_code,
                        ReconcileReason::Other("evm_uncertain_timeout".to_string()),
                        true,
                    );
                    let _ = ApiWithdrawRepo::mark_broadcast_uncertain_reconciled(
                        &self.ctx.api_transaction_pool()?,
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
                    &self.ctx.api_transaction_pool()?,
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
                    &self.ctx.api_transaction_pool()?,
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
            Err(err) if shared_mechanism::is_tron_missing_confirmed_and_pending_error(&err) => {
                if !Self::should_fail_expired_tron_missing_confirmed(&req) {
                    info!(
                        trade_no = %trade_no,
                        tx_hash = %req.tx_hash.as_deref().unwrap_or_default(),
                        settle_buffer_ms = Self::TRON_EXPIRED_RAW_MISSING_CONFIRMED_SETTLE_BUFFER_MS,
                        source = "shadow_withdraw_worker",
                        "Tron tx missing from confirmed and pending pools; keep observing until raw tx expiration"
                    );
                    return Ok(());
                }

                warn!(
                    trade_no = %trade_no,
                    tx_hash = %req.tx_hash.as_deref().unwrap_or_default(),
                    settle_buffer_ms = Self::TRON_EXPIRED_RAW_MISSING_CONFIRMED_SETTLE_BUFFER_MS,
                    source = "shadow_withdraw_worker",
                    "Tron tx missing from confirmed and pending pools after raw tx expiration; marking manual handling"
                );
                let rows_affected = ApiWithdrawRepo::update_api_withdraw_status_and_err(
                    &self.ctx.api_transaction_pool()?,
                    &req.trade_no,
                    wallet_database::entities::api_withdraw::ApiWithdrawStatus::SendingTxFailed,
                    ErrCode::TransactionOnChainException,
                    "Tron broadcast missing from both confirmed and pending pools after timeout; manual intervention required",
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
                let stage_rows = ApiWithdrawRepo::set_failure_stage(
                    &self.ctx.api_transaction_pool()?,
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
            self.resource_reservations.release(&trade_no);
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

        if !self.check_withdraw_amount(&withdraw).await? {
            info!(
                trade_no = %trade_no,
                reason_code = "amount_check_failed",
                source = "shadow_withdraw_worker",
                "Withdraw amount insufficient, failing build directly"
            );
            return Err(ServiceError::Business(crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::InsufficientBalance(
                    crate::error::business::chain::InsufficientBalanceDetail::new()
                        .from_addr(withdraw.from_addr.clone())
                        .to_addr(withdraw.to_addr.clone())
                        .chain_code(withdraw.chain_code.clone())
                        .token_addr(withdraw.token_addr.to_string())
                        .value(withdraw.value.clone())
                        .reason(
                            "withdraw amount is insufficient; balance is below requested value",
                        ),
                ),
            )));
        }

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
                self.clear_build_slot_after_claim(trade_no).await?;
                return Ok(());
            }

            // 7. 获取并更新 nonce
            let nonce =
                self.get_nonce(&fresh_withdraw.from_addr, &fresh_withdraw.chain_code).await?;
            info!(trade_no = %trade_no, nonce = %nonce, source = "shadow_withdraw_worker", "Retrieved nonce");

            nonce
        };

        // 先占位 build slot，防止同一 trade_no 在构建期间被重复推进。
        let build_slot_rows =
            ApiWithdrawRepo::update_building_at(&self.ctx.api_transaction_pool()?, trade_no)
                .await?;
        if build_slot_rows == 0 {
            info!(
                trade_no = %trade_no,
                source = "shadow_withdraw_worker",
                "Build slot already claimed or recently updated, skipping BuildTx"
            );
            return Ok(());
        }

        // ====== phase 2: 网络执行 ======
        // 通过Context获取Handles实例，然后获取私钥管理器
        let handles = self.ctx.get_handles_arc().await?;
        let private_key_manager = handles.get_global_private_key_manager();
        let private_key =
            private_key_manager.get_private_key(&withdraw.from_addr, &withdraw.chain_code).await?;
        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Retrieved private key from manager");

        // 9. 生成转账请求
        // ⚠️ nonce 只在 phase 1 分配，这里直接传入
        let transfer_req = self.gen_transfer_req(&withdraw, nonce).await?;
        info!(trade_no = %trade_no, nonce = %nonce, source = "shadow_withdraw_worker", "Generated transfer request with nonce");

        // 10. 构建交易。公共包装负责 guarded endpoint 并发、timeout、成功/失败记录。
        let Some((tx_hash, raw_tx, fee_str)) =
            crate::infrastructure::chain_rpc_guard::run_guarded_with_ctx_and_log(
                self.ctx,
                &withdraw.chain_code,
                trade_no,
                "build",
                "shadow_withdraw_worker",
                crate::infrastructure::chain_rpc_guard::ChainRpcFailureLog::new()
                    .trade_no(trade_no)
                    .chain_code(&withdraw.chain_code)
                    .source("shadow_withdraw_worker")
                    .op("build"),
                ApiTransDomain::build_transfer_raw(
                    &self.ctx,
                    transfer_req,
                    Some(private_key), // 私钥管理
                ),
            )
            .await?
        else {
            return Ok(());
        };
        info!(trade_no = %trade_no, tx_hash = %tx_hash, fee = %fee_str, source = "shadow_withdraw_worker", "Built transfer raw transaction successfully");

        // ====== phase 3: 提交不可逆事实 ======
        {
            // 11. 立即将tx_hash、raw_tx和nonce存储到数据库
            let raw_tx_str = wallet_utils::serde_func::serde_to_string(&raw_tx)?;
            let rows_affected = ApiWithdrawRepo::update_after_build(
                &self.ctx.api_transaction_pool()?,
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
                self.clear_build_slot_after_claim(trade_no).await?;
                return Ok(());
            }

            info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Updated tx_hash, raw_tx and nonce to database successfully");

            // 直接调用 try_advance 进行点对点唤醒
            self.scanner.try_advance(&withdraw.trade_no).await;
        }

        Ok(())
    }

    async fn check_withdraw_amount(
        &self,
        withdraw: &ApiWithdrawEntity,
    ) -> Result<bool, ServiceError> {
        let coin = ApiCoinDomain::get_coin_by_token_key_exact(
            self.ctx,
            &withdraw.chain_code,
            withdraw.token_addr.clone(),
        )
        .await?;

        let token_key = if coin.token_address.is_native() {
            AssetTokenKey::Native
        } else {
            coin.token_address.clone()
        };

        let adapter: std::sync::Arc<dyn crate::domain::api_wallet::adapter::tx::Tx + Send + Sync> =
            ApiChainAdapterFactory::get_transaction_adapter_with_ctx(self.ctx, &withdraw.chain_code)
                .await?;
        let balance = adapter.balance_token_key(&withdraw.from_addr, token_key).await?;
        let balance = unit::format_to_string(balance, coin.decimals)?;

        tracing::info!(
            trade_no = %withdraw.trade_no,
            balance = %balance,
            requested_value = %withdraw.value,
            token_addr = %withdraw.token_addr,
            source = "shadow_withdraw_worker",
            "Withdraw amount check completed"
        );

        Ok(!Self::is_withdraw_amount_shortage(&balance, &withdraw.value)?)
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

            if shared_mechanism::tracks_broadcast_uncertain_state(&fresh_withdraw.chain_code)
                && fresh_withdraw.broadcast_uncertain_since_at.is_some()
                && fresh_withdraw.last_broadcast_at.is_none()
                && fresh_withdraw.transaction_time.is_none()
                && fresh_withdraw.tx_exec_receipt_uploaded_at.is_none()
                && fresh_withdraw.err_code.is_none()
            {
                info!(
                    observability_event = "shadow_broadcast_skipped",
                    reason = "broadcast_uncertain_in_progress",
                    trade_no = %trade_no,
                    tx_hash = %fresh_withdraw.tx_hash.as_deref().unwrap_or_default(),
                    nonce = fresh_withdraw.nonce,
                    source = "shadow_withdraw_worker",
                    "Skip Broadcast: uncertain state in progress; recover should proceed"
                );
                return Ok(());
            }

            if shared_mechanism::is_evm_chain(&fresh_withdraw.chain_code) {
                match ApiTransDomain::nonce(
                    &self.ctx,
                    &fresh_withdraw.from_addr,
                    &fresh_withdraw.chain_code,
                )
                .await
                {
                    Ok(chain_nonce) => {
                        if shared_mechanism::should_force_align_nonce_gap(
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

                            let nonce_engine = get_nonce_engine_with_ctx(self.ctx)?;
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

        // 6. 反序列化raw_tx
        let raw_tx = wallet_utils::serde_func::serde_from_str(
            &withdraw.raw_tx.as_deref().unwrap_or_default(),
        )?;
        info!(trade_no = %trade_no, tx_hash = %withdraw.tx_hash.as_deref().unwrap_or_default(), source = "shadow_withdraw_worker", "Deserialized raw_tx successfully");

        // 7. 广播交易
        info!(trade_no = %trade_no, tx_hash = %withdraw.tx_hash.as_deref().unwrap_or_default(), source = "shadow_withdraw_worker", "Starting to broadcast transaction");
        let tx_resp = crate::infrastructure::chain_rpc_guard::run_guarded_with_ctx_and_log(
            self.ctx,
            &withdraw.chain_code,
            trade_no,
            "broadcast",
            "shadow_withdraw_worker",
            crate::infrastructure::chain_rpc_guard::ChainRpcFailureLog::new()
                .trade_no(trade_no)
                .tx_hash(withdraw.tx_hash.as_deref().unwrap_or_default())
                .chain_code(&withdraw.chain_code)
                .source("shadow_withdraw_worker")
                .op("broadcast"),
            ApiTransDomain::broadcast_transfer(
                &self.ctx,
                &withdraw.chain_code,
                raw_tx,
                withdraw.tx_hash.as_deref(),
            ),
        )
        .await?
        .flatten();

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
                        Self::tron_actual_resource_json(consumer)?
                    } else {
                        "0".to_string()
                    };

                    let rows_affected = ApiWithdrawRepo::mark_broadcast_executed(
                        &self.ctx.api_transaction_pool()?,
                        &withdraw.trade_no,
                    )
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
                    self.resource_reservations.release(&withdraw.trade_no);
                }

                Ok(())
            }
            None => {
                info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Transaction broadcast result is uncertain");
                if !shared_mechanism::tracks_broadcast_uncertain_state(&withdraw.chain_code) {
                    return Ok(());
                }
                let had_uncertain_since = withdraw.broadcast_uncertain_since_at.is_some();
                let rows_affected = ApiWithdrawRepo::mark_broadcast_uncertain_attempt(
                    &self.ctx.api_transaction_pool()?,
                    trade_no,
                )
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
                    "Broadcast uncertain state recorded"
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
            &self.ctx.api_transaction_pool()?,
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
                use crate::infrastructure::nonce::nonce_engine::get_nonce_engine_with_ctx;

                let nonce_engine = get_nonce_engine_with_ctx(self.ctx)?;
                let nonce = nonce_engine.allocate_nonce(from_addr, chain_code).await?;
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
        let sn = self.ctx.get_sn();
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
            self.ctx,
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
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter_with_ctx(self.ctx, "tron").await?;
        let mut params = ApiBaseTransferReq::new(from, to, value, "tron");
        params.with_token(token_key.to_chain_token_option(), decimals, symbol);
        let fee = adapter.estimate_fee(params, main_symbol).await?;
        let details: TronFeeDetails = wallet_utils::serde_func::serde_from_str(&fee)?;
        Ok(details)
    }

    async fn estimate_tron_fee_details_for_withdraw(
        &self,
        req: &ApiWithdrawEntity,
    ) -> Result<TronFeeDetails, ServiceError> {
        let main_coin = ApiCoinDomain::get_coin_by_token_key_exact(
            self.ctx,
            &req.chain_code,
            AssetTokenKey::Native,
        )
        .await?;
        let (token_symbol, token_key, token_decimals) = if req.token_addr.is_contract() {
            let token_coin = ApiCoinDomain::get_coin_by_token_key_exact(
                self.ctx,
                &req.chain_code,
                req.token_addr.clone(),
            )
            .await?;
            (token_coin.symbol, token_coin.token_address, token_coin.decimals)
        } else {
            (main_coin.symbol.clone(), AssetTokenKey::Native, main_coin.decimals)
        };

        self.estimate_tron_fee_details(
            &req.from_addr,
            &req.to_addr,
            &req.value,
            &token_symbol,
            &main_coin.symbol,
            token_key,
            token_decimals,
        )
        .await
    }

    async fn estimate_fee_snapshot_for_withdraw(
        &self,
        req: &ApiWithdrawEntity,
    ) -> Result<(String, String), ServiceError> {
        let main_coin = ApiCoinDomain::get_coin_by_token_key_exact(
            self.ctx,
            &req.chain_code,
            AssetTokenKey::Native,
        )
        .await?;
        let transfer_coin = if req.token_addr.is_contract() {
            ApiCoinDomain::get_coin_by_token_key_exact(
                self.ctx,
                &req.chain_code,
                req.token_addr.clone(),
            )
            .await?
        } else {
            main_coin.clone()
        };
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter_with_ctx(self.ctx, &req.chain_code)
                .await?;
        let mut params =
            ApiBaseTransferReq::new(&req.from_addr, &req.to_addr, &req.value, &req.chain_code);
        params.with_token(
            transfer_coin.token_address.to_chain_token_option(),
            transfer_coin.decimals,
            &transfer_coin.symbol,
        );
        let snapshot = adapter.estimate_fee_snapshot(params, &main_coin.symbol).await?;
        let estimated_resource_consume = match (snapshot.energy, snapshot.bandwidth) {
            (Some(energy), Some(bandwidth)) => {
                Self::tron_fee_estimate_resource_json(energy, bandwidth)?
            }
            _ => "{}".to_string(),
        };
        Ok((snapshot.amount, estimated_resource_consume))
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
                        self.ctx,
                        &delegation.chain_code,
                        "withdraw_resource_delegation",
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
        if !shared_mechanism::is_tron_chain(&delegation.chain_code) {
            return Err(ServiceError::Parameter(format!(
                "resource delegation only supports tron, got {}",
                delegation.chain_code
            )));
        }

        let trx_amount = parse_resource_delegation_native_trx_units(&delegation.native_amount)?;
        let resource = Self::tron_resource_name(delegation.resource_type);
        let chain = ChainAdapterFactory::get_tron_adapter_with_ctx(self.ctx).await?;
        let _guard = crate::infrastructure::chain_rpc_guard::acquire_if_guarded_with_ctx(
            self.ctx,
            &delegation.chain_code,
        )
        .await;
        let signer = resolve_resource_delegation_signer(self.ctx, delegation).await?;

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
        let tx_resp = ApiTransDomain::broadcast_transfer(
            &self.ctx,
            &delegation.chain_code,
            raw_tx,
            Some(&tx_hash),
        )
        .await?;

        let Some(tx) = tx_resp else {
            return Err(ServiceError::Parameter(
                "resource delegation broadcast result uncertain".to_string(),
            ));
        };
        if tx.tx_hash != tx_hash {
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
        let chain = ChainAdapterFactory::get_tron_adapter_with_ctx(self.ctx).await?;
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

        let handles = self.ctx.get_handles_arc().await?;
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
        if err.is_transient_db_pool_timeout() {
            warn!(
                resource_trade_no = %resource_trade_no,
                error = %err,
                source = "shadow_withdraw_worker",
                "Skip marking resource delegation failed: local DB pool timeout is retryable infrastructure noise"
            );
            return Ok(());
        }

        let err_code = if err.is_network_error() { "ERR_6005" } else { "ERR_6008" };
        ApiResourceDelegationRepo::mark_failed_if_unfinished(
            &self.ctx.api_resource_pool()?,
            resource_trade_no,
            err_code,
            &err.to_string(),
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        Ok(())
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

        if shared_mechanism::is_evm_chain(&withdraw.chain_code) {
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

        if shared_mechanism::is_evm_chain(&withdraw.chain_code)
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
            &self.ctx,
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

        if err.is_transient_db_pool_timeout() {
            warn!(
                trade_no = %trade_no,
                error = %err,
                source = "shadow_withdraw_worker",
                "Skip marking withdraw tx failed: local DB pool timeout is retryable infrastructure noise"
            );
            return Ok(());
        }

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

        let error_msg = err.to_string();

        if matches!(stage, WithdrawFailureStage::Build) && withdraw.raw_tx.is_none() {
            if let Some(err_code) = Self::build_failure_err_code(&err) {
                let rows_affected = ApiWithdrawRepo::mark_build_failed_if_unfinished(
                    &self.ctx.api_transaction_pool()?,
                    trade_no,
                    err_code,
                    &error_msg,
                )
                .await
                .map_err(|db_err: wallet_database::Error| {
                    error!(trade_no = %trade_no, error = %db_err, source = "shadow_withdraw_worker", "Failed to mark BuildTx terminal failure");
                    ServiceError::Database(db_err.into())
                })?;

                info!(
                    trade_no = %trade_no,
                    rows_affected = %rows_affected,
                    err_code = %err_code,
                    source = "shadow_withdraw_worker",
                    "Marked BuildTx terminal failure"
                );

                if rows_affected > 0 {
                    self.scanner.try_advance(trade_no).await;
                }

                return Ok(());
            }
        }

        // BuildTx 失败只需要释放 build slot，让 scanner 后续重试。
        // 这里不写失败事实，避免把可重试的构建失败误记成终态失败。
        if matches!(stage, WithdrawFailureStage::Build) && withdraw.raw_tx.is_none() {
            let rows_affected = ApiWithdrawRepo::clear_building_at(&self.ctx.api_transaction_pool()?, trade_no)
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

        let broadcast_failure_stage = match stage {
            WithdrawFailureStage::Broadcast => {
                broadcast_mechanism::BroadcastFailureStage::Broadcast
            }
            _ => broadcast_mechanism::BroadcastFailureStage::Other,
        };

        let broadcast_failure_decision = broadcast_mechanism::classify_broadcast_failure(
            &err,
            broadcast_mechanism::BroadcastFailureFacts::new(
                &withdraw.chain_code,
                withdraw.raw_tx.is_some(),
                withdraw.tx_hash.is_some(),
                withdraw.last_broadcast_at.is_some(),
                broadcast_failure_stage,
                broadcast_mechanism::DuplicateBroadcastAcceptance::AllowWithoutLocalTxFacts,
            ),
        );

        if let Some(rebuild_reason) = broadcast_failure_decision.raw_rebuild_reason() {
            let rows_affected =
                ApiWithdrawRepo::invalidate_raw_tx(&self.ctx.api_transaction_pool()?, trade_no, None, None, None)
                    .await
                    .map_err(|db_err: wallet_database::Error| {
                        error!(trade_no = %trade_no, error = %db_err, source = "shadow_withdraw_worker", "{}", rebuild_reason.db_error_message());
                        ServiceError::Database(db_err.into())
                    })?;
            info!(
                trade_no = %trade_no,
                rows_affected = %rows_affected,
                error = %err,
                source = "shadow_withdraw_worker",
                "{}", rebuild_reason.log_message()
            );
            if rows_affected > 0 {
                self.scanner.try_advance(trade_no).await;
            }
            return Ok(());
        }

        if let Some(accepted_reason) = broadcast_failure_decision.accepted_reason() {
            if let Some(message) = accepted_reason.pre_self_heal_log_message() {
                info!(
                    trade_no = %trade_no,
                    source = "shadow_withdraw_worker",
                    "{}", message
                );
            }

            if accepted_reason.requires_nonce_self_heal() {
                nonce_recovery_mechanism::try_nonce_self_heal(
                    self.ctx,
                    trade_no,
                    &withdraw.from_addr,
                    &withdraw.chain_code,
                    &error_msg,
                    "shadow_withdraw_worker",
                )
                .await?;
            }

            let rows_affected = ApiWithdrawRepo::mark_broadcast_executed(&self.ctx.api_transaction_pool()?, trade_no)
                .await
                .map_err(|db_err: wallet_database::Error| {
                    error!(trade_no = %trade_no, error = %db_err, source = "shadow_withdraw_worker", "{}", accepted_reason.db_error_message());
                    ServiceError::Database(db_err.into())
                })?;
            info!(
                trade_no = %trade_no,
                rows_affected = %rows_affected,
                error = %err,
                source = "shadow_withdraw_worker",
                "{}", accepted_reason.log_message()
            );
            self.resource_reservations.release(trade_no);
            self.scanner.try_advance(trade_no).await;
            return Ok(());
        }

        // 处理 nonce too low 错误
        if broadcast_failure_decision.should_fail_after_nonce_self_heal() {
            info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Detected nonce too low error, syncing nonce from chain");

            nonce_recovery_mechanism::try_nonce_self_heal(
                self.ctx,
                trade_no,
                &withdraw.from_addr,
                &withdraw.chain_code,
                &error_msg,
                "shadow_withdraw_worker",
            )
            .await?;

            let rows_affected = ApiWithdrawRepo::update_api_withdraw_status_and_err(
                &self.ctx.api_transaction_pool()?,
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
                    &self.ctx.api_transaction_pool()?,
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
                let stage_rows = match ApiWithdrawRepo::set_failure_stage(
                    &self.ctx.api_transaction_pool()?,
                    trade_no,
                    stage,
                )
                .await
                {
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

    async fn clear_build_slot_after_claim(&self, trade_no: &str) -> Result<(), ServiceError> {
        Self::clear_build_slot_after_claim_in_pool(&self.ctx.api_transaction_pool()?, trade_no)
            .await
    }

    async fn clear_build_slot_after_claim_in_pool(
        pool: &ApiTransactionDbPool,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        let rows_affected = ApiWithdrawRepo::clear_building_at(pool, trade_no)
            .await
            .map_err(|db_err: wallet_database::Error| {
                error!(trade_no = %trade_no, error = %db_err, source = "shadow_withdraw_worker", "Failed to clear build slot after BuildTx early exit");
                ServiceError::Database(db_err.into())
            })?;

        info!(
            trade_no = %trade_no,
            rows_affected = %rows_affected,
            source = "shadow_withdraw_worker",
            "BuildTx early exit cleared build slot"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ResourceBudget, ShadowWithdrawWorker};
    use crate::{
        context::api_wallet_backend::ApiWalletBackend, domain::api_wallet::adapter::tx::RawTx,
        error::system::SystemError,
        infrastructure::api_trans::resource_apply_outcome::PlatformApplyOutcome,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use std::{fs, path::PathBuf, sync::Arc};
    use tempfile::tempdir;
    use tokio::sync::{OnceCell, mpsc};
    use wallet_chain_interact::{BillResourceConsume, tron::operations::RawTransactionParams};
    use wallet_database::{
        ApiWalletDbPool, SqliteContext,
        entities::{
            api_resource_delegation::{
                ApiResourceDelegationOperationType, ApiResourceDelegationRecoverStatus,
                NewApiResourceDelegation,
            },
            api_resource_gate::{ApiResourceBlockReason, ApiResourceDependencyType},
            api_resource_type::ApiResourceType,
            api_trade_type::ApiTradeType,
            api_wallet::ApiWalletType,
            api_withdraw::{
                ApiWithdrawEntity, ApiWithdrawStatus, ErrCode, PendingWithdrawResourceBudget,
            },
            asset_token_key::AssetTokenKey,
        },
        repositories::api_wallet::{
            resource_delegation::ApiResourceDelegationRepo, wallet::ApiWalletRepo,
            withdraw::ApiWithdrawRepo,
        },
    };
    use wallet_transport_backend::{
        request::{
            DeviceDeleteReq, KeysInitReq,
            api_wallet::{
                address::ExpandAddressCompleteReq,
                swap::{ApiInitSwapReq, ApiInitSwapResponse},
                wallet::{
                    AppIdImportRechargeWalletReq, AppIdImportReq, AppIdUidUsageReq, BindAppIdReq,
                },
            },
        },
        response_vo::api_wallet::wallet::{
            AppIdUidUsageRes as ApiWalletAppIdUidUsageRes, KeysUidCheckRes, QueryUidBindInfoRes,
            QueryWalletActivationInfoResp, UidStatus,
        },
    };

    #[derive(Default)]
    struct NoopApiWalletBackend;

    #[async_trait]
    impl ApiWalletBackend for NoopApiWalletBackend {
        async fn wallet_bind_appid(
            &self,
            _: BindAppIdReq,
        ) -> Result<(), crate::error::service::ServiceError> {
            Ok(())
        }
        async fn init_api_wallet(
            &self,
            _: AppIdImportReq,
        ) -> Result<(), crate::error::service::ServiceError> {
            Ok(())
        }
        async fn old_keys_init(
            &self,
            _: KeysInitReq,
        ) -> Result<(), crate::error::service::ServiceError> {
            Ok(())
        }
        async fn appid_import(
            &self,
            _: AppIdImportReq,
        ) -> Result<(), crate::error::service::ServiceError> {
            Ok(())
        }
        async fn appid_import_recharge_wallet(
            &self,
            _: AppIdImportRechargeWalletReq,
        ) -> Result<(), crate::error::service::ServiceError> {
            Ok(())
        }
        async fn keys_uid_check(
            &self,
            _: &str,
        ) -> Result<KeysUidCheckRes, crate::error::service::ServiceError> {
            Ok(KeysUidCheckRes { uid: String::new(), status: UidStatus::NormalWallet })
        }
        async fn query_uid_bind_info(
            &self,
            _: &str,
        ) -> Result<QueryUidBindInfoRes, crate::error::service::ServiceError> {
            Ok(QueryUidBindInfoRes {
                app_id: String::new(),
                org_id: String::new(),
                bind_status: false,
                sn: String::new(),
            })
        }
        async fn query_wallet_activation_info(
            &self,
            _: &str,
        ) -> Result<QueryWalletActivationInfoResp, crate::error::service::ServiceError> {
            Ok(QueryWalletActivationInfoResp(vec![]))
        }
        async fn appid_uid_usage(
            &self,
            _: AppIdUidUsageReq,
        ) -> Result<ApiWalletAppIdUidUsageRes, crate::error::service::ServiceError> {
            Ok(ApiWalletAppIdUidUsageRes { used: false })
        }
        async fn expand_address_complete(
            &self,
            _: ExpandAddressCompleteReq,
        ) -> Result<(), crate::error::service::ServiceError> {
            Ok(())
        }
        async fn appid_withdrawal_wallet_change(
            &self,
            _: &str,
            _: &str,
        ) -> Result<(), crate::error::service::ServiceError> {
            Ok(())
        }
        async fn init_swap(
            &self,
            _: &ApiInitSwapReq,
        ) -> Result<ApiInitSwapResponse, crate::error::service::ServiceError> {
            Ok(ApiInitSwapResponse { success: true, code: None, msg: None, data: None })
        }
        async fn device_delete(
            &self,
            _: &DeviceDeleteReq,
        ) -> Result<Option<()>, crate::error::service::ServiceError> {
            Ok(None)
        }
    }

    async fn test_ctx() -> &'static crate::context::Context {
        static CTX: tokio::sync::OnceCell<&'static crate::context::Context> =
            tokio::sync::OnceCell::const_new();

        *CTX.get_or_init(|| async {
            let config = crate::config::Config::new(
                r#"
app_code: "test"
crypto:
  aes_key: "1234567890abcdef"
  aes_iv: "abcdef1234567890"
backend_api:
  dev_url: "http://127.0.0.1:9"
  test_url: "http://127.0.0.1:9"
  prod_url: "http://127.0.0.1:9"
aggregate_api:
  dev_url: "http://127.0.0.1:9"
  test_url: "http://127.0.0.1:9"
  prod_url: "http://127.0.0.1:9"
oss:
  access_key_id: "id"
  access_key_secret: "secret"
  bucket_name: "bucket"
  endpoint: "oss-endpoint"
"#,
            )
            .expect("parse test config");

            let mut dir = PathBuf::from(std::env::temp_dir());
            dir.push("wallet-api-withdraw-shadow-worker-tests");
            fs::create_dir_all(&dir).expect("ensure temp dir");

            let dirs = crate::dirs::Dirs::new(&dir.to_string_lossy()).expect("create dirs");
            crate::context::init_context_with_api_wallet_backend(
                "wallet_api_shadow_withdraw_test_sn",
                "unittest",
                dirs,
                None,
                config,
                Arc::new(NoopApiWalletBackend::default()),
            )
            .await
            .expect("init test context")
        })
        .await
    }

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
            estimated_transaction_fee: None,
            estimated_resource_consume: None,
            fee_estimated_at: None,
            transaction_time: None,
            block_height: None,
            notes: None,
            post_tx_count: 0,
            post_confirm_tx_count: 0,
            err_code: None,
            err_msg: None,
            resource_check_at: None,
            resource_gate_released_at: None,
            resource_gate_result: None,
            resource_block_reason: None,
            resource_dependency_trade_no: None,
            resource_dependency_type: None,
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
            out_order_id: None,
            client_id: None,
            create_time: None,
        }
    }

    #[test]
    fn withdraw_fee_estimate_snapshot_resource_json_uses_product_keys() {
        let json = ShadowWithdrawWorker::tron_fee_estimate_resource_json(345, 210)
            .expect("serialize resource consume");
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["energy"], 345);
        assert_eq!(value["bandwidth"], 210);
        assert!(value.get("energy_used").is_none());
        assert!(value.get("net_used").is_none());
    }

    #[test]
    fn withdraw_resource_gate_subtracts_db_pending_budget_from_chain_available() {
        let available = ShadowWithdrawWorker::effective_resource_after_pending_withdraws(
            13_532,
            690,
            PendingWithdrawResourceBudget { energy: 13_532, bandwidth: 345 },
        );

        assert_eq!(available, ResourceBudget::new(0, 345));
    }

    #[test]
    fn withdraw_resource_gate_pending_budget_saturates_at_zero() {
        let available = ShadowWithdrawWorker::effective_resource_after_pending_withdraws(
            326,
            100,
            PendingWithdrawResourceBudget { energy: 13_532, bandwidth: 345 },
        );

        assert_eq!(available, ResourceBudget::new(0, 0));
    }

    #[test]
    fn withdraw_resource_gate_waits_when_platform_delegate_dependency_exists() {
        let mut withdraw = base_withdraw();
        withdraw.resource_block_reason = Some(ApiResourceBlockReason::NeedPlatformDelegate);
        withdraw.resource_dependency_type = Some(ApiResourceDependencyType::PlatformDelegate);
        withdraw.resource_dependency_trade_no = Some(withdraw.trade_no.clone());

        assert!(ShadowWithdrawWorker::is_waiting_platform_delegate_result(&withdraw));
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

    fn tron_raw_tx_json(expiration_ms: i64) -> String {
        let raw = RawTransactionParams {
            raw_data: serde_json::json!({ "expiration": expiration_ms }).to_string(),
            raw_data_hex: "00".to_string(),
            tx_id: "txid".to_string(),
            signature: vec![],
        };
        let raw_tx =
            RawTx::Tron(raw, BillResourceConsume { net_used: 0, energy_used: 0 }, String::new());
        wallet_utils::serde_func::serde_to_string(&raw_tx).expect("serialize tron raw tx")
    }

    #[test]
    fn withdraw_tron_missing_confirmed_waits_until_raw_expiration() {
        let mut withdraw = base_withdraw();
        withdraw.chain_code = "tron".to_string();
        withdraw.raw_tx = Some(tron_raw_tx_json(Utc::now().timestamp_millis() + 60_000));

        assert!(!ShadowWithdrawWorker::should_fail_expired_tron_missing_confirmed(&withdraw));
    }

    #[test]
    fn withdraw_tron_missing_confirmed_fails_after_raw_expiration_buffer() {
        let mut withdraw = base_withdraw();
        withdraw.chain_code = "tron".to_string();
        withdraw.raw_tx = Some(tron_raw_tx_json(Utc::now().timestamp_millis() - 31_000));

        assert!(ShadowWithdrawWorker::should_fail_expired_tron_missing_confirmed(&withdraw));
    }

    #[test]
    fn withdraw_tron_missing_confirmed_requires_tron_raw_tx() {
        let mut withdraw = base_withdraw();
        withdraw.chain_code = "sol".to_string();
        withdraw.raw_tx = Some(tron_raw_tx_json(Utc::now().timestamp_millis() - 31_000));

        assert!(!ShadowWithdrawWorker::should_fail_expired_tron_missing_confirmed(&withdraw));

        withdraw.chain_code = "tron".to_string();
        withdraw.raw_tx = None;

        assert!(!ShadowWithdrawWorker::should_fail_expired_tron_missing_confirmed(&withdraw));
    }

    #[test]
    fn withdraw_platform_resource_apply_amounts_match_collect_semantics() {
        let amounts =
            crate::infrastructure::api_trans::resource_amount::energy_shortfall_to_apply_amounts(
                "800", 400.0,
            )
            .expect("parse amount");

        assert_eq!(amounts.resource_amount, 800.0);
        assert_eq!(amounts.native_token_amount, 2);
    }

    #[test]
    fn withdraw_energy_delegation_amount_requires_real_energy_shortfall() {
        assert_eq!(ShadowWithdrawWorker::energy_delegation_shortfall_amount(14650, 52769), None);
        assert_eq!(
            ShadowWithdrawWorker::energy_delegation_shortfall_amount(14650, 0),
            Some("14650".to_string())
        );
    }

    #[test]
    fn withdraw_platform_apply_success_accepts_delegation_trade_no() {
        assert_eq!(
            ShadowWithdrawWorker::platform_apply_outcome(true, Some("DL_1".to_string())),
            PlatformApplyOutcome::AcceptedWithResourceTradeNo("DL_1".to_string())
        );
        assert_eq!(
            ShadowWithdrawWorker::platform_apply_outcome(true, Some(" DL_1 ".to_string())),
            PlatformApplyOutcome::AcceptedWithResourceTradeNo("DL_1".to_string())
        );
        assert_eq!(
            ShadowWithdrawWorker::platform_apply_outcome(false, None),
            PlatformApplyOutcome::Rejected
        );
    }

    #[test]
    fn withdraw_platform_apply_success_without_delegation_trade_no_waits_original_order_result() {
        assert_eq!(
            ShadowWithdrawWorker::platform_apply_outcome(true, None),
            PlatformApplyOutcome::AcceptedWithOriginalTradeNo
        );
        assert_eq!(
            ShadowWithdrawWorker::platform_apply_outcome(true, Some("  ".to_string())),
            PlatformApplyOutcome::AcceptedWithOriginalTradeNo
        );
    }

    #[test]
    fn withdraw_resource_delegation_retries_rpc_auth_unauthorized() {
        let err = crate::error::service::ServiceError::TransportBackend(
            wallet_transport_backend::Error::ApiBackend(401, Some("Unauthorized".to_string())),
        );

        assert!(
            crate::infrastructure::api_trans::resource_rpc_auth::should_retry_after_rpc_auth_error(
                &err
            )
        );
    }

    #[test]
    fn withdraw_amount_shortage_detects_balance_below_requested_value() {
        assert!(
            ShadowWithdrawWorker::is_withdraw_amount_shortage("1.109998", "1.109999")
                .expect("compare")
        );
        assert!(
            !ShadowWithdrawWorker::is_withdraw_amount_shortage("1.109999", "1.109999")
                .expect("compare")
        );
    }

    #[test]
    fn withdraw_build_failure_classifies_balance_errors_as_terminal() {
        let insufficient_balance = crate::error::service::ServiceError::Business(
            crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::insufficient_balance(),
            ),
        );
        let insufficient_fee = crate::error::service::ServiceError::Business(
            crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::InsufficientFeeBalance,
            ),
        );
        let retryable_build_error =
            crate::error::service::ServiceError::Parameter("network will retry".to_string());

        assert_eq!(
            ShadowWithdrawWorker::build_failure_err_code(&insufficient_balance),
            Some(ErrCode::BalanceInsufficient)
        );
        assert_eq!(
            ShadowWithdrawWorker::build_failure_err_code(&insufficient_fee),
            Some(ErrCode::FeeInsufficient)
        );
        assert_eq!(ShadowWithdrawWorker::build_failure_err_code(&retryable_build_error), None);
    }

    #[tokio::test]
    async fn clear_build_slot_after_claim_releases_building_at() {
        let dir = tempdir().expect("tempdir");
        let dir_path = dir.path().to_string_lossy().to_string();

        let collect_ctx = SqliteContext::new(&dir_path, Some("api_transaction.db"))
            .await
            .expect("init api_transaction.db");
        let collect_pool = collect_ctx.into_transaction_db_pool().expect("transaction pool");

        let trade_no = "W_clear_build_slot_after_claim";
        ApiWithdrawRepo::upsert_api_withdraw(
            &collect_pool,
            "uid",
            "withdraw",
            "from",
            "to",
            "1",
            "digest",
            "eth",
            None,
            "USDT",
            trade_no,
            None,
            None,
            None,
            ApiTradeType::Withdraw,
            0,
            None,
            ApiWithdrawStatus::SendingTx,
            ApiWithdrawStatus::SendingTx,
            "0",
            "0",
            None,
            None,
        )
        .await
        .expect("insert withdraw");

        let claimed = ApiWithdrawRepo::update_building_at(&collect_pool, trade_no)
            .await
            .expect("claim build slot");
        assert_eq!(claimed, 1);

        ShadowWithdrawWorker::clear_build_slot_after_claim_in_pool(&collect_pool, trade_no)
            .await
            .expect("clear build slot");

        let persisted = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &collect_pool,
            trade_no,
            ApiTradeType::Withdraw,
        )
        .await
        .expect("load withdraw");
        assert!(persisted.building_at.is_none());
    }

    #[tokio::test]
    async fn withdraw_platform_delegate_retryable_error_releases_build_slot() {
        let dir = tempdir().expect("tempdir");
        let dir_path = dir.path().to_string_lossy().to_string();

        let transaction_ctx = SqliteContext::new(&dir_path, Some("api_transaction.db"))
            .await
            .expect("init api_transaction.db");
        let transaction_pool =
            transaction_ctx.into_transaction_db_pool().expect("transaction pool");
        let resource_ctx = SqliteContext::new(&dir_path, Some("api_resource.db"))
            .await
            .expect("init api_resource.db");
        let resource_pool = resource_ctx.into_api_resource_db_pool().expect("resource pool");

        ApiResourceDelegationRepo::upsert(
            &resource_pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid",
                "wd_platform_delegate_retry",
                ApiTradeType::Withdraw,
                ApiResourceDelegationOperationType::Delegate,
                "tron",
                "withdraw_owner",
                "receiver",
                ApiResourceType::Energy,
                "10",
                "10",
            ),
        )
        .await
        .expect("insert platform delegation");
        ApiResourceDelegationRepo::mark_task_ack_sent(&resource_pool, "wd_platform_delegate_retry")
            .await
            .expect("mark ack");
        ApiResourceDelegationRepo::claim_build_slot(&resource_pool, "wd_platform_delegate_retry")
            .await
            .expect("claim build slot");

        ShadowWithdrawWorker::schedule_resource_delegation_rebuild_retry_in_pool(
            &resource_pool,
            "wd_platform_delegate_retry",
            &crate::error::service::ServiceError::Parameter("retryable test error".to_string()),
        )
        .await
        .expect("schedule retry");

        let task = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &resource_pool,
            "wd_platform_delegate_retry",
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
    async fn withdraw_resource_apply_identity_uses_wallet_app_and_merchant_ids() {
        let test_ctx = test_ctx().await;
        let wallet_pool = test_ctx.api_wallet_pool().expect("wallet pool");

        ApiWalletRepo::upsert(
            &wallet_pool,
            "wallet_uid",
            "api_wallet",
            "wallet_address",
            b"phrase",
            b"seed",
            ApiWalletType::Withdrawal,
            None,
            "sn_1",
            0,
        )
        .await
        .expect("insert wallet");
        ApiWalletRepo::update_app_id(&wallet_pool, "wallet_address", Some("app_1"))
            .await
            .expect("set app id");
        ApiWalletRepo::update_merchant_id(&wallet_pool, "wallet_address", "merchant_1")
            .await
            .expect("set merchant id");

        let (intent_tx, _intent_rx) = mpsc::channel(1);
        let scanner =
            Arc::new(crate::infrastructure::api_trans::withdraw::shadow::ShadowScanner::new(
                test_ctx,
                crate::infrastructure::api_trans::withdraw::shadow::ScannerConfig::default(),
                intent_tx,
                None,
            ));
        let worker = ShadowWithdrawWorker::new(test_ctx, scanner);

        let (app_id, org_id) =
            worker.resolve_resource_apply_identity("wallet_uid").await.expect("resolve identity");

        assert_eq!(app_id, "app_1");
        assert_eq!(org_id, "merchant_1");
    }
}
