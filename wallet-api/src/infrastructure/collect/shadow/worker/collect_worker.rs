// collect/shadow/worker/collect_worker.rs

// Architecture Rule:
// - Broadcast success MUST only update last_broadcast_at
// - transaction_time is an irreversible on-chain confirmation fact
// - Only Scanner / Shadow Recovery may write transaction_time
use std::sync::Arc;

use chrono::Utc;
use rust_decimal::{Decimal, prelude::ToPrimitive};
use tokio::sync::Semaphore;
use tracing::{error, info, warn};
use wallet_database::{
    CollectDbPool, CoreDbPool,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus, ErrCode},
    repositories::api_wallet::{
        account::ApiAccountRepo, collect::ApiCollectRepo, nonce::ApiNonceRepo,
        wallet::ApiWalletRepo,
    },
};
use wallet_transport_backend::request::api_wallet::strategy::ChainConfig;
use wallet_types::chain::chain::ChainCode;
use wallet_utils::{conversion, unit};

// 从crate::response_vo导入必要的Fee类型
use crate::{
    domain::api_wallet::{trans::ApiTransDomain, wallet::ApiWalletDomain},
    error::{business::api_wallet::trans::TransError, system::SystemError},
    request::api_wallet::trans::ApiTransferReq,
    response_vo::{CommonFeeDetails, EthereumFeeDetails, FeeDetailsVo, TronFeeDetails},
};

use crate::{
    domain::api_wallet::{
        adapter_factory::ApiChainAdapterFactory, chain::ApiChainTransDomain, coin::ApiCoinDomain,
        strategy::StrategyDomain,
    },
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    infrastructure::collect::process_collect_tx_send::AddressLockManager,
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
/// - global_sem 只限制外部世界并发
/// - 允许失败和重试
///
/// Phase 3: DB commit (with address lock)
/// - 持锁写事实，保证原子性
/// - 只写事实，不做决策
/// - 写事实后必须调用 try_advance 唤醒 Scanner
use crate::infrastructure::collect::shadow::{CollectIntent, ScannerConfig, ShadowScanner};

pub struct ShadowCollectWorker {
    /// 数据库连接池
    collect_pool: CollectDbPool,
    core_pool: CoreDbPool,
    /// 地址锁管理器，保护地址级并发
    address_locks: Arc<AddressLockManager>,
    /// 全局信号量，控制 RPC / 链上执行的并发度
    global_sem: Arc<Semaphore>,
    /// ShadowScanner 引用，用于直接调用 try_advance
    scanner: Arc<ShadowScanner>,
}

impl ShadowCollectWorker {
    /// 创建新的 Shadow Collect Worker
    pub fn new(
        pool: CollectDbPool,
        core_pool: CoreDbPool,
        address_locks: Arc<AddressLockManager>,
        global_sem: Arc<Semaphore>,
        scanner: Arc<ShadowScanner>,
    ) -> Self {
        Self { collect_pool: pool, core_pool, address_locks, global_sem, scanner }
    }

    /// 处理单个 Command
    pub async fn handle(&self, cmd: ShadowCollectCommand) -> Result<(), ServiceError> {
        // 提取 trade_no 用于日志
        let trade_no = match &cmd {
            ShadowCollectCommand::BuildTx(trade_no) => trade_no,
            ShadowCollectCommand::Broadcast(trade_no) => trade_no,
        };

        info!(trade_no = %trade_no, command = ?cmd, source = "shadow_worker_v2", "Received shadow collect command");

        match cmd {
            ShadowCollectCommand::BuildTx(trade_no) => self.process_build_tx(trade_no).await,
            ShadowCollectCommand::Broadcast(trade_no) => self.process_broadcast(trade_no).await,
        }
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

        // 3. 交易恢复：如果已有 tx_hash 且 transaction_time 为空，检查链上状态
        // ⚠️ IMPORTANT:
        // Recover logic here MUST NOT write transaction_time.
        // On-chain confirmation fact is owned by Scanner / Shadow Recovery ONLY.
        // This logic is for BuildTx stage only, DO NOT reuse in other stages.
        if req.tx_hash.is_some() && req.transaction_time.is_none() {
            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Found existing tx_hash, attempting recovery");
            match self.recover_tx(&req).await? {
                Some(tx_resp) => {
                    info!(trade_no = %trade_no, tx_hash = %tx_resp.tx_hash, source = "shadow_worker_v2", "Transaction recovery successful");

                    // ⚠️ Recover FACT COMPLETION RULE
                    //
                    // If on-chain final result is observed via tx_hash:
                    // 1. broadcast MUST have happened (behavior fact)
                    // 2. final result MUST be known (chain fact)
                    //
                    // Therefore, Recover MUST atomically ensure:
                    // - last_broadcast_at IS NOT NULL
                    // - transaction_time IS NOT NULL
                    //
                    // Writing only transaction_time without last_broadcast_at
                    // is a FACT MODEL VIOLATION and will break scanner predicates.
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

                    // last_broadcast_at 使用与 transaction_time 相同的值
                    // 这是一个显式不变量：last_broadcast_at = transaction_time
                    let last_broadcast_at = transaction_time.clone();

                    let rows_affected =
                        ApiCollectRepo::confirm_onchain_transaction_fact_with_recover(
                            &self.collect_pool,
                            &req.trade_no,
                            &tx_resp.tx_hash,
                            &last_broadcast_at,
                            &transaction_time,
                            &tx_resp.fee,
                            &resource_consume,
                        )
                        .await
                        .map_err(|e| ServiceError::Database(e.into()))?;

                    // 显式处理幂等情况：事务已被其他并发/广播确认
                    if rows_affected == 0 {
                        info!(
                            trade_no = %trade_no,
                            tx_hash = %tx_resp.tx_hash,
                            source = "shadow_worker_v2",
                            "confirm_transaction skipped during recovery: transaction already confirmed (idempotent hit)"
                        );
                    } else {
                        // 直接调用 try_advance 进行点对点唤醒
                        self.scanner.try_advance(&req.trade_no).await;
                    }

                    return Ok(());
                }
                None => {
                    info!(trade_no = %trade_no, source = "shadow_worker_v2", "Transaction recovery result is uncertain, will retry");
                    return Ok(());
                }
            }
        }

        // 4. 解析执行地址 - 在执行期解析，支持重试
        let exec_to_addr = self.resolve_collect_to_addr(&req).await?;
        info!(trade_no = %trade_no, exec_to_addr = %exec_to_addr, source = "shadow_worker_v2", "Resolved execution address");

        if req.to_addr.is_empty() {
            req.to_addr = exec_to_addr.clone();
            // 更新数据库中的to_addr
            ApiCollectRepo::update_api_collect_to_addr(
                &self.collect_pool,
                &req.trade_no,
                &exec_to_addr,
            )
            .await?;
            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Updated to_addr in database");
        }

        // 5. 检查手续费
        //
        // ⚠️ IMPORTANT:
        // Fee insufficient is NOT a retryable failure.
        // It invalidates the current build facts and must go through invalidate_raw_tx.
        // Do NOT introduce any logic that only sets build_blocked_at.
        if !self.check_fee(&req).await? {
            info!(
                trade_no = %trade_no,
                source = "shadow_worker_v2",
                "Fee insufficient, invalidating current build attempt"
            );

            // 🔒 事实作废：原子性地清空 raw_tx、tx_hash 并设置 build_blocked_at
            // NOTE: InsufficientBalance represents a build invalidation reason,
            // NOT an execution failure.
            let affected = ApiCollectRepo::invalidate_raw_tx(
                &self.collect_pool,
                &req.trade_no,
                Some(ApiCollectStatus::InsufficientBalance),
            )
            .await?;

            if affected == 0 {
                info!(
                    trade_no = %trade_no,
                    source = "shadow_worker_v2",
                    "Transaction already invalidated or no raw_tx to invalidate, skip"
                );
            } else {
                // 直接调用 try_advance 进行点对点唤醒
                self.scanner.try_advance(&req.trade_no).await;
            }

            return Ok(());
        }
        info!(trade_no = %trade_no, source = "shadow_worker_v2", "Fee check passed");

        // 6. 检查交易摘要 - 仍然使用 req.to_addr（原始输入）
        if !self.check_digest(&req).await? {
            tracing::error!(trade_no=%trade_no, "collect_tx:send: 交易摘要验证失败");
            return Err(ServiceError::Business(
                ApiWalletError::Trans(TransError::TransactionDigestVerificationFailed).into(),
            ));
        }
        info!(trade_no = %trade_no, source = "shadow_worker_v2", "Transaction digest verification passed");

        // ====== phase 1: 锁内 · 快速检查 ======
        // ⚠️ 锁内禁止任何网络调用、sleep、await RPC
        let nonce = {
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
                return Ok(());
            }

            // 获取并更新 nonce - 使用唯一入口 upsert_and_get_api_nonce
            // ⚠️ nonce 获取必须在锁内，确保同一地址的 nonce 串行化
            let nonce = self.get_nonce(&fresh_req.from_addr, &fresh_req.chain_code).await?;
            info!(trade_no = %trade_no, nonce = %nonce, source = "shadow_worker_v2", "Retrieved nonce");

            nonce
        };
        // 🔓 锁在这里已经释放

        // ====== phase 2: 锁外 · 网络执行 ======
        // 获取全局信号量许可，控制RPC/链上执行的并发度
        let _global_guard = self
            .global_sem
            .acquire()
            .await
            .map_err(|_| ServiceError::System(SystemError::SemaphoreClosed))?;
        info!(trade_no = %trade_no, source = "shadow_worker_v2", "Acquired global semaphore");

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

            // 立即将tx_hash和raw_tx存储到数据库
            // 注意：使用序列化而非格式化，避免格式问题
            let raw_tx_str = wallet_utils::serde_func::serde_to_string(&raw_tx)?;
            let rows_affected = ApiCollectRepo::update_after_build(
                &self.collect_pool,
                &req.trade_no,
                &tx_hash,
                &raw_tx_str,
                &fee,
            )
            .await?;

            // 显式处理幂等情况：如果影响行数为0，表示raw_tx已存在或被并发写入
            if rows_affected == 0 {
                info!(trade_no = %trade_no, source = "shadow_worker_v2", "update_after_build skipped: raw_tx already exists (idempotent hit)");
                return Ok(());
            }

            info!(trade_no = %trade_no, source = "shadow_worker_v2", "Updated tx_hash and raw_tx to database successfully");

            // 直接调用 try_advance 进行点对点唤醒
            self.scanner.try_advance(&req.trade_no).await;
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

            fresh_req
        };
        // 🔓 锁在这里已经释放

        // ====== phase 2: 锁外 · 网络执行 ======
        // 获取全局信号量许可，控制RPC/链上执行的并发度
        let _global_guard = self
            .global_sem
            .acquire()
            .await
            .map_err(|_| ServiceError::System(SystemError::SemaphoreClosed))?;
        info!(trade_no = %trade_no, source = "shadow_worker_v2", "Acquired global semaphore for broadcast");

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
                        self.scanner.try_advance(&fresh_req.trade_no).await;
                    }
                }

                Ok(())
            }
            None => {
                info!(trade_no = %trade_no, source = "shadow_worker_v2", "Transaction broadcast result is uncertain");
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
    async fn check_fee(&self, req: &ApiCollectEntity) -> Result<bool, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 开始检查手续费, 发送方={}, 接收方={}, 金额={}, 代币地址={:?}", 
            req.from_addr, req.to_addr, req.value, req.token_addr);

        // 查询主币信息
        let chain_code: ChainCode = req.chain_code.as_str().try_into()?;
        let main_coin = ApiChainTransDomain::main_coin(&req.chain_code).await?;
        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 主币信息: 币种={}, 小数位数={}", main_coin.symbol, main_coin.decimals);

        // 确定代币信息
        let (token_symbol, token, token_decimals) = if let Some(token) = req.token_addr.clone() {
            if token.is_empty() {
                (main_coin.symbol.clone(), None, main_coin.decimals)
            } else {
                let token_coin =
                    ApiCoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_addr.clone())
                        .await?;
                tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 代币信息: 币种={}, 代币地址={:?}, 小数位数={}", 
                    token_coin.symbol, token_coin.token_address, token_coin.decimals);
                (token_coin.symbol, token_coin.token_address, token_coin.decimals)
            }
        } else {
            (main_coin.symbol.clone(), None, main_coin.decimals)
        };

        // 估算手续费
        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 估算手续费参数: 发送方={}, 接收方={}, 金额={}, 主币={}, 代币={}, 代币小数位数={}", 
            req.from_addr, req.to_addr, req.value, main_coin.symbol, token_symbol, token_decimals);
        let fee_str = self
            .estimate_fee(
                &req.from_addr,
                &req.to_addr,
                &req.value,
                chain_code,
                &token_symbol,
                &main_coin.symbol,
                token,
                token_decimals,
            )
            .await?;
        let fee = conversion::decimal_from_str(&fee_str)?;
        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 估算手续费完成: {}", fee_str);

        // 查询资产主币余额
        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 查询主币余额");
        let balance =
            self.query_balance(&req.from_addr, chain_code, None, main_coin.decimals).await?;
        let balance = conversion::decimal_from_str(&balance)?;
        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 主币余额查询完成: {}", balance);

        // 计算需要的总金额
        let need = if req.token_addr.is_some() {
            // 代币交易只需要手续费
            tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 代币交易，只需要手续费");
            fee
        } else {
            // 主币交易需要手续费+转账金额
            let value = conversion::decimal_from_str(&req.value)?;
            tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 主币交易，需要手续费+转账金额, 转账金额={}", value);
            fee + value
        };

        tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 手续费检查结果 - 可用余额: {}, 需要金额: {}, 手续费: {}", balance, need, fee);

        if fee > Decimal::from(0) && balance < need {
            tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 手续费不足，需要请求补充");

            // 计算需要补充的手续费
            // NOTE: fee_to_upload is calculated for Fee module consumption.
            // Shadow worker must not trigger fee upload.
            let mut fee_to_upload = if let Some(f) = fee.to_f64() { f } else { 0.0 };
            if chain_code == ChainCode::Ethereum || chain_code == ChainCode::BnbSmartChain {
                fee_to_upload = fee_to_upload * 2.0;
                tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 以太坊/BSC网络，手续费翻倍: {}", fee_to_upload);
            }

            // 直接返回 false，不更新状态
            Ok(false)
        } else {
            tracing::info!(trade_no=%req.trade_no, source = "shadow_worker_v2", "collect_tx:send: 手续费充足，继续交易");
            Ok(true)
        }
    }

    pub(crate) async fn resolve_withdraw_from_addr(
        pool: &CoreDbPool,
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
        token_address: Option<String>,
        decimals: u8,
    ) -> Result<String, ServiceError> {
        tracing::info!(owner_address=%owner_address, chain_code=%chain_code.to_string(), token_address=%token_address.as_deref().unwrap_or(""), 
            source = "shadow_worker_v2", "collect_tx:send: 查询余额");

        // Log token_address before moving it to adapter.balance
        let token_address_log = token_address.clone();
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter(&chain_code.to_string()).await?;
        let balance = adapter.balance(&owner_address, token_address).await?;
        let amount = unit::format_to_string(balance, decimals)?;

        tracing::info!(owner_address=%owner_address, chain_code=%chain_code.to_string(), token_address=%token_address_log.as_deref().unwrap_or(""), 
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
        token_address: Option<String>,
        decimals: u8,
    ) -> Result<String, ServiceError> {
        // TODO: 可优化速度
        let start_time = std::time::Instant::now();
        tracing::info!(from=%from, to=%to, value=%value, chain_code=%chain_code.to_string(), symbol=%symbol,
            main_symbol=%main_symbol, token_address=%token_address.as_deref().unwrap_or(""), 
            source = "shadow_worker_v2", "collect_tx:send: 估算交易手续费开始");

        let adapter_start = std::time::Instant::now();
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter(&chain_code.to_string()).await?;
        tracing::info!(chain_code=%chain_code.to_string(), duration_ms=%adapter_start.elapsed().as_millis(), source = "shadow_worker_v2", "collect_tx:send: 获取适配器完成");

        let params_start = std::time::Instant::now();
        let mut params = ApiBaseTransferReq::new(from, to, value, &chain_code.to_string());
        params.with_token(token_address, decimals, symbol);
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
        let coin =
            ApiCoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_addr.clone()).await?;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 获取币种信息成功, symbol={}, token_address={:?}, decimals={}", 
            coin.symbol, coin.token_address, coin.decimals);

        // 创建基础转账请求 - 使用exec_to_addr而非req.to_addr
        let mut params =
            ApiBaseTransferReq::new(&req.from_addr, exec_to_addr, &req.value, &req.chain_code);
        let token_address = if coin.token_address.is_none() {
            None
        } else {
            let s = coin.token_address.unwrap();
            if s.is_empty() { None } else { Some(s) }
        };
        params.with_token(token_address, coin.decimals, &coin.symbol);
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 创建基础转账请求成功");

        // 获取钱包密码
        let passwd = ApiWalletDomain::get_passwd().await?;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 获取钱包密码成功");

        let transfer_req = ApiTransferReq { base: params, password: passwd, nonce };
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
                // 对于以太坊类链，使用数据库原子upsert确保nonce的唯一性和递增
                // ⚠️ INVARIANT:
                // This method MUST guarantee DB-level atomic CAS for nonce allocation.
                // Any refactor breaking this invariant will cause nonce duplication.
                let nonce = wallet_database::repositories::api_wallet::nonce::ApiNonceRepo::upsert_and_get_api_nonce(
                    &self.collect_pool,
                    from_addr,
                    chain_code,
                    0 // 0 表示使用当前 nonce 并递增
                ).await?;
                info!(from_addr = %from_addr, chain_code = %chain_code, nonce = %nonce, source = "shadow_worker_v2", "Retrieved nonce using atomic upsert");
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

        // 更新数据库状态为失败
        let error_msg = format!("{}", err);
        match err.retry_policy() {
            wallet_transport::errors::RetryPolicy::Never => {
                let err_code = if err.is_network_error() {
                    ErrCode::NetworkException
                } else {
                    ErrCode::SDKInternalError
                };

                let rows_affected = wallet_database::repositories::api_wallet::collect::ApiCollectRepo::update_api_collect_status_and_err(
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
                    self.scanner.try_advance(&trade_no).await;
                }

                // 注意：Shadow Worker 是执行者，不是裁决者
                // 不设置 finished_at，因为链上事实尚未闭环
                // 只有 Scanner/Shadow Recovery 才能设置终态
            }
            wallet_transport::errors::RetryPolicy::Delay => {}
        }

        Ok(())
    }
}
