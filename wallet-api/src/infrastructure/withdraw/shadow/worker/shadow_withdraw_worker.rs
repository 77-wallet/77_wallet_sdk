// withdraw/shadow/worker/shadow_withdraw_worker.rs
use std::sync::Arc;

use chrono::Utc;
use tracing::{error, info};
use wallet_database::{
    CollectDbPool, CoreDbPool,
    entities::api_withdraw::{ApiWithdrawEntity, ErrCode},
    repositories::api_wallet::{nonce::ApiNonceRepo, withdraw::ApiWithdrawRepo},
};
use wallet_types::chain::chain::ChainCode;

use crate::{
    domain::api_wallet::{trans::ApiTransDomain, wallet::ApiWalletDomain},
    error::{
        business::api_wallet::{ApiWalletError, trans::TransError},
        service::ServiceError,
        system::SystemError,
    },
    infrastructure::withdraw::shadow::ShadowScanner,
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
/// - global_sem 只限制外部世界并发
/// - 允许失败和重试
///
/// Phase 3: Irreversible fact commit
/// - 提交不可逆事实
/// - 更新数据库状态
/// - 唤醒扫描器
///
/// 🔒 核心原则：
/// - nonce 从"动态信息"升级为"已裁决事实"
/// - global_sem 作为 RPC 压力阀
pub struct ShadowWithdrawWorker {
    pool: CollectDbPool,
    core_pool: CoreDbPool,
    global_sem: Arc<tokio::sync::Semaphore>,
    /// ShadowScanner 引用，用于直接调用 try_advance
    scanner: Arc<ShadowScanner>,
}

impl ShadowWithdrawWorker {
    pub fn new(
        pool: CollectDbPool,
        core_pool: CoreDbPool,
        global_sem: Arc<tokio::sync::Semaphore>,
        scanner: Arc<ShadowScanner>,
    ) -> Self {
        Self { pool, core_pool, global_sem, scanner }
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
            self.handle_withdraw_tx_failed(&trade_no, err).await?;
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
            if fresh_req.tx_hash.is_none() || fresh_req.transaction_time.is_some() {
                info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "tx_hash empty or transaction_time exists, skipping Recover");
                return Ok(());
            }

            fresh_req
        };

        // ====== phase 2: 锁外 · 网络执行 ======
        // 获取全局信号量许可，控制RPC/链上执行的并发度
        let _global_guard = self
            .global_sem
            .acquire()
            .await
            .map_err(|_| ServiceError::System(SystemError::SemaphoreClosed))?;
        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Acquired global semaphore");

        // 执行恢复交易
        match self.recover_tx(&req).await? {
            Some(tx_resp) => {
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
            None => {
                info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Transaction recover result is uncertain");
            }
        }

        Ok(())
    }

    /// 执行 BuildTx Command - 外层wrapper，确保所有错误都被捕获
    async fn process_build_tx(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Processing BuildTx command");

        // 使用内层函数来捕获所有错误
        if let Err(err) = self.process_build_tx_inner(&trade_no).await {
            error!(trade_no = %trade_no, error = %err, source = "shadow_withdraw_worker", "BuildTx inner failed, handling error");
            self.handle_withdraw_tx_failed(&trade_no, err).await?;
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

        // ====== phase 2: 网络执行 ======
        // 8. 获取全局信号量许可，控制RPC/链上执行的并发度
        let _global_guard = self
            .global_sem
            .acquire()
            .await
            .map_err(|_| ServiceError::System(SystemError::SemaphoreClosed))?;
        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Acquired global semaphore");

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
            self.handle_withdraw_tx_failed(&trade_no, err).await?;
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

            // 3. 检查是否已有raw_tx和tx_hash
            if fresh_withdraw.tx_hash.is_none() || fresh_withdraw.raw_tx.is_none() {
                error!(trade_no = %trade_no, source = "shadow_withdraw_worker", "No raw_tx or tx_hash found");
                return Err(ServiceError::Business(
                    ApiWalletError::Trans(crate::error::business::api_wallet::trans::TransError::BuildWithdrawTransactionFailed("Missing transaction data".to_string())).into(),
                ));
            }
        }

        // ====== phase 2: 网络执行 ======
        // 8. 获取全局信号量许可，控制RPC/链上执行的并发度
        let _global_guard = self
            .global_sem
            .acquire()
            .await
            .map_err(|_| ServiceError::System(SystemError::SemaphoreClosed))?;
        info!(trade_no = %trade_no, source = "shadow_withdraw_worker", "Acquired global semaphore");

        // 6. 反序列化raw_tx
        let raw_tx = wallet_utils::serde_func::serde_from_str(
            &withdraw.raw_tx.as_deref().unwrap_or_default(),
        )?;
        info!(trade_no = %trade_no, tx_hash = %withdraw.tx_hash.as_deref().unwrap_or_default(), source = "shadow_withdraw_worker", "Deserialized raw_tx successfully");

        // 7. 广播交易
        info!(trade_no = %trade_no, tx_hash = %withdraw.tx_hash.as_deref().unwrap_or_default(), source = "shadow_withdraw_worker", "Starting to broadcast transaction");
        let tx_resp = ApiTransDomain::broadcast_transfer(&withdraw.chain_code, raw_tx).await?;

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
                // 对于以太坊类链，使用数据库原子upsert确保nonce的唯一性和递增
                // ⚠️ INVARIANT:
                // This method MUST guarantee DB-level atomic CAS for nonce allocation.
                // Any refactor breaking this invariant will cause nonce duplication.
                let nonce = ApiNonceRepo::upsert_and_get_api_nonce(
                    &self.pool, from_addr, chain_code,
                    0, // 0 表示使用当前 nonce 并递增
                )
                .await?;
                info!(from_addr = %from_addr, chain_code = %chain_code, nonce = %nonce, source = "shadow_withdraw_worker", "Retrieved nonce from database via atomic upsert");
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
        let mut params =
            ApiBaseTransferReq::new(&req.from_addr, &req.to_addr, &req.value, &req.chain_code);

        // 设置代币转账参数
        if let Some(token_addr) = &req.token_addr {
            if !token_addr.is_empty() {
                tracing::info!(trade_no=%req.trade_no, token_address=?token_addr, "[提币] 设置代币转账参数");
                // 假设默认小数位为 18
                params.with_token(Some(token_addr.clone()), 18, &req.symbol);
            }
        }

        tracing::info!(trade_no=%req.trade_no, "[提币] 获取钱包密码");
        let passwd = ApiWalletDomain::get_passwd().await?;

        tracing::info!(trade_no=%req.trade_no, nonce=%nonce, "[提币] 转账请求生成完成");
        Ok(ApiTransferReq { base: params, password: passwd, nonce })
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
        err: ServiceError,
    ) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, error = %err, source = "shadow_withdraw_worker", "Handling withdraw tx failed");

        // 🔒 事实保护：检查是否已存在成功事实
        let withdraw = self.get_withdraw_entity(trade_no).await?;
        if withdraw.transaction_time.is_some() {
            info!(
                trade_no = %trade_no,
                source = "shadow_withdraw_worker",
                "Skip mark failed: transaction already confirmed (monotonicity constraint)"
            );
            return Ok(());
        }

        // 更新数据库状态为失败
        let error_msg = format!("{}", err);

        // 根据错误类型确定错误码
        let err_code = if err.is_network_error() {
            ErrCode::NetworkException
        } else {
            ErrCode::SDKInternalError
        };

        let rows_affected = ApiWithdrawRepo::update_api_withdraw_status_and_err(
            &self.pool,
            trade_no,
            wallet_database::entities::api_withdraw::ApiWithdrawStatus::SendingTxFailed,
            err_code, // err_code - 根据错误类型设置
            &error_msg,
        )
        .await
        .map_err(|db_err: wallet_database::Error| {
            error!(trade_no = %trade_no, error = %db_err, source = "shadow_withdraw_worker", "Failed to update status to failed");
            ServiceError::Database(db_err.into())
        })?;
        info!(trade_no = %trade_no, rows_affected = %rows_affected, source = "shadow_withdraw_worker", "Updated status to failed");

        // 只有第一次写入失败事实才发送 Tick
        if rows_affected > 0 {
            // 直接调用 try_advance 进行点对点唤醒
            self.scanner.try_advance(trade_no).await;
        }

        // 注意：Shadow Worker 是执行者，不是裁决者
        // 不设置 finished_at，因为链上事实尚未闭环
        // 只有 Scanner/Shadow Recovery 才能设置终态

        Ok(())
    }
}
