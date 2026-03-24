use std::sync::Arc;

// collect/shadow/worker/side_effect_worker.rs
//
// SideEffect Worker 负责处理所有外部依赖的副作用操作
// - 发送结果确认 (SendResultAck)
// - 上传服务费记录 (UploadServiceFee)
// - 上传交易执行回执 (UploadTxExecReceipt)
// - 其他未来可能的副作用操作
//
// SideEffect Worker 约束：
// - 只处理外部依赖的操作，不处理链相关操作
// - 所有操作必须有幂等性保护
// - 统一执行流程：事实读取 → 幂等判断 → 执行 + 写结果事实
// - 可随时被 kill-9，不影响系统一致性
//
// Invariants:
// 1. *_uploaded_at.is_some() => *_attempted_at.is_some()
// 2. SideEffectWorker never writes business status
// 3. Failure can never overwrite success
use rust_decimal::prelude::ToPrimitive as _;
use tracing::{error, info, warn};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool,
    entities::{api_coin::ApiCoinEntity, asset_token_key::AssetTokenKey},
};
use wallet_transport_backend::request::api_wallet::transaction::ServiceFeeUploadReq;
use wallet_types::chain::chain::ChainCode;
use wallet_utils::conversion;

use crate::{
    domain::api_wallet::{chain::ApiChainTransDomain, coin::ApiCoinDomain},
    error::service::ServiceError,
    infrastructure::api_trans::collect::shadow::ShadowAdvancer,
    request::api_wallet::trans::ApiBaseTransferReq,
};

/// SideEffect Worker Command 结构
/// 只表达："对某个 trade_no 执行某个确定的副作用动作"
#[derive(Debug)]
pub enum SideEffectCommand {
    /// 发送订单确认
    SendOrderAck(String),
    /// 发送结果确认
    SendResultAck(String),
    /// 上传服务费记录
    UploadServiceFee(String),
    /// 上传交易执行回执
    UploadTxExecReceipt(String),
    /// 发送手续费结果确认
    SendTxFeeResAck(String),
}

impl SideEffectCommand {
    /// 从 SideEffectCommand 生成对应的 RunningKey
    pub fn to_running_key(
        &self,
    ) -> crate::infrastructure::api_trans::collect::shadow::dispatcher::RunningKey {
        match self {
            SideEffectCommand::SendOrderAck(trade_no) => {
                crate::infrastructure::api_trans::collect::shadow::dispatcher::RunningKey::SendOrderAck(
                    trade_no.clone(),
                )
            }
            SideEffectCommand::SendResultAck(trade_no) => {
                crate::infrastructure::api_trans::collect::shadow::dispatcher::RunningKey::SendResultAck(
                    trade_no.clone(),
                )
            }
            SideEffectCommand::UploadServiceFee(trade_no) => {
                crate::infrastructure::api_trans::collect::shadow::dispatcher::RunningKey::UploadServiceFee(
                    trade_no.clone(),
                )
            }
            SideEffectCommand::UploadTxExecReceipt(trade_no) => {
                crate::infrastructure::api_trans::collect::shadow::dispatcher::RunningKey::UploadTxExecReceipt(
                    trade_no.clone(),
                )
            }
            SideEffectCommand::SendTxFeeResAck(trade_no) => {
                crate::infrastructure::api_trans::collect::shadow::dispatcher::RunningKey::SendTxFeeResAck(
                    trade_no.clone(),
                )
            }
        }
    }
}

/// SideEffect Worker
/// 处理所有外部依赖的副作用操作
#[derive(Clone)]
pub struct SideEffectWorker {
    /// 数据库连接池
    pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
    /// ShadowAdvancer 引用，用于统一推进执行
    advancer: Arc<ShadowAdvancer>,
}

impl SideEffectWorker {
    /// 创建新的 SideEffect Worker
    pub fn new(
        pool: ApiTransactionDbPool,
        core_pool: ApiWalletDbPool,
        advancer: Arc<ShadowAdvancer>,
    ) -> Self {
        Self { pool, core_pool, advancer }
    }

    /// 从数据库中获取归集交易信息
    async fn get_collect_entity(
        &self,
        trade_no: &str,
    ) -> Result<wallet_database::entities::api_collect::ApiCollectEntity, ServiceError> {
        let entity = wallet_database::repositories::api_wallet::collect::ApiCollectRepo::get_api_collect_by_trade_no(&self.pool, trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        Ok(entity)
    }

    /// 解析出款地址
    async fn resolve_withdraw_from_addr(
        &self,
        req: &wallet_database::entities::api_collect::ApiCollectEntity,
    ) -> Result<String, ServiceError> {
        info!(trade_no = %req.trade_no, source = "side_effect_worker", "Resolving withdrawal address");

        // 1. 根据from_addr + chain_code查询account
        let account = match wallet_database::repositories::api_wallet::account::ApiAccountRepo::find_one_by_address_chain_code(
            &req.from_addr,
            &req.chain_code,
            &self.core_pool,
        )
        .await?
        {
            Some(account) => account,
            None => {
                error!(trade_no = %req.trade_no, from_addr = %req.from_addr, chain_code = %req.chain_code, source = "side_effect_worker", "Account not found");
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
        let wallet =
            match wallet_database::repositories::api_wallet::wallet::ApiWalletRepo::find_by_address(
                &self.core_pool.clone(),
                &account.wallet_address,
            )
            .await?
            {
                Some(wallet) => wallet,
                None => {
                    error!(trade_no = %req.trade_no, wallet_address = %account.wallet_address, source = "side_effect_worker", "Wallet not found");
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
            error!(trade_no = %req.trade_no, wallet_address = %account.wallet_address, source = "side_effect_worker", "Wallet not bound to address");
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
            wallet_database::repositories::api_wallet::wallet::ApiWalletRepo::find_by_address(
                &self.core_pool.clone(),
                &bind_address,
            )
            .await?
        else {
            error!(trade_no = %req.trade_no, bind_address = %bind_address, source = "side_effect_worker", "Withdrawal wallet not found");
            return Err(ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::Wallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                ),
            )));
        };

        // 3. 查询用户提币策略
        let strategy =
            crate::domain::api_wallet::strategy::StrategyDomain::query_withdraw_strategy(
                &withdraw_wallet.uid,
            )
            .await?;
        info!(trade_no = %req.trade_no, source = "side_effect_worker", "Retrieved withdrawal strategy successfully");

        // 4. 根据chain_code查询链配置
        let chain_config = match strategy
            .chain_configs
            .into_iter()
            .find(|config| config.chain_code == req.chain_code)
        {
            Some(config) => config,
            None => {
                error!(trade_no = %req.trade_no, chain_code = %req.chain_code, source = "side_effect_worker", "Chain config not found");
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

        info!(trade_no = %req.trade_no, exec_to_addr = %exec_to_addr, source = "side_effect_worker", "Resolved withdrawal address successfully");
        Ok(exec_to_addr)
    }

    /// 处理单个副作用命令
    pub async fn handle(&self, cmd: SideEffectCommand) -> Result<(), ServiceError> {
        // 提取 trade_no 用于日志
        let trade_no = match &cmd {
            SideEffectCommand::SendOrderAck(trade_no) => trade_no,
            SideEffectCommand::SendResultAck(trade_no) => trade_no,
            SideEffectCommand::UploadServiceFee(trade_no) => trade_no,
            SideEffectCommand::UploadTxExecReceipt(trade_no) => trade_no,
            SideEffectCommand::SendTxFeeResAck(trade_no) => trade_no,
        };

        let trade_no_clone = trade_no.to_string();
        let trade_no_for_async = trade_no_clone.clone();
        let self_clone = self.clone();

        match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            async move {
                info!(trade_no = %trade_no_for_async, command = ?cmd, source = "side_effect_worker", "Received side effect command");

                // 幂等保护：检查是否已终态
                // finished_at 一旦存在，世界已经结束，后面发生的一切都只是日志
                if let Ok(collect) = self_clone.get_collect_entity(&trade_no_for_async).await {
                    if collect.finished_at.is_some() {
                        info!(trade_no = %trade_no_for_async, command = ?cmd, source = "side_effect_worker", "Collect already finished, skipping side effect");
                        return Ok(());
                    }
                }

                match cmd {
                    SideEffectCommand::SendOrderAck(trade_no) => self_clone.process_order_ack(trade_no).await,
                    SideEffectCommand::SendResultAck(trade_no) => self_clone.process_result_ack(trade_no).await,
                    SideEffectCommand::UploadServiceFee(trade_no) => {
                        self_clone.process_upload_service_fee(trade_no).await
                    }
                    SideEffectCommand::UploadTxExecReceipt(trade_no) => {
                        self_clone.process_tx_exec_receipt(trade_no).await
                    }
                    SideEffectCommand::SendTxFeeResAck(trade_no) => {
                        self_clone.process_tx_fee_res_ack(trade_no).await
                    }
                }
            }
        ).await {
            Ok(result) => result,
            Err(_) => {
                error!(trade_no = %trade_no_clone, source = "side_effect_worker", "SideEffectWorker timeout after 30 seconds");
                Err(ServiceError::Timeout)
            }
        }
    }

    /// 处理发送订单确认
    async fn process_order_ack(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "side_effect_worker", "Processing Order ACK command");

        // 获取交易信息
        let req = self.get_collect_entity(&trade_no).await?;

        // 幂等保护：检查是否已发送订单 ACK
        if req.order_ack_sent_at.is_some() {
            info!(trade_no = %trade_no, source = "side_effect_worker", "Order ACK already sent, skipping");
            return Ok(());
        }

        // 标记订单 ACK 尝试
        info!(trade_no = %trade_no, source = "side_effect_worker", "Marking order ACK as attempted");
        wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_order_ack_attempted(
            &self.pool,
            &trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        info!(trade_no = %trade_no, source = "side_effect_worker", "Order ACK marked as attempted successfully");

        // 获取backend_api
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        // 发送Order ACK
        match backend_api
            .trans_event_ack(
                &wallet_transport_backend::request::api_wallet::transaction::TransEventAckReq::new(
                    &trade_no,
                    wallet_transport_backend::request::api_wallet::transaction::TransType::Col,
                    wallet_transport_backend::request::api_wallet::transaction::TransAckType::Tx,
                ),
            )
            .await
        {
            Ok(_) => {
                info!(trade_no = %trade_no, "Order ACK sent successfully");
                // 成功路径：标记订单 ACK 已发送
                wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_order_ack_sent(
                        &self.pool,
                        &trade_no,
                    ).await
                    .map_err(|e| {
                        error!(trade_no = %trade_no, error = %e, "Failed to mark order ACK sent");
                        ServiceError::Database(e.into())
                    })?;

                // 发送 Tick 通知，触发扫描
                // 直接调用 try_advance 进行点对点唤醒
                self.advancer.try_advance(&trade_no).await;
            }
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, "Failed to send Order ACK");
                // 失败路径：只保留 attempted 状态，让 Scanner 重试
                return Err(e.into());
            }
        }

        Ok(())
    }

    /// 处理发送结果确认
    async fn process_result_ack(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "side_effect_worker", "Processing Result ACK command");

        // 获取交易信息
        let req = self.get_collect_entity(&trade_no).await?;

        // 幂等保护：检查是否已发送结果确认
        if req.result_ack_sent_at.is_some() {
            if req.finished_at.is_none() && req.transaction_time.is_some() {
                // 兼容历史半完成事实：result_ack 已写但 finished 未写（例如 kill -9）
                info!(
                    trade_no = %trade_no,
                    source = "side_effect_worker",
                    "Result ACK already sent but collect not finished; repairing finished_at"
                );
                wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_chain_finished(
                        &self.pool,
                        &trade_no,
                    )
                    .await
                    .map_err(|e| ServiceError::Database(e.into()))?;
                self.advancer.try_advance(&trade_no).await;
            } else if req.transaction_time.is_none() {
                warn!(
                    trade_no = %trade_no,
                    source = "side_effect_worker",
                    "Result ACK already sent but transaction_time is NULL; skip repairing finished_at"
                );
            }

            info!(
                trade_no = %trade_no,
                source = "side_effect_worker",
                "Result ACK already sent, skipping"
            );
            return Ok(());
        }

        // ✅ 强顺序屏障：TX_RES ACK 只能在已收到并持久化 AWM_ORDER_TRANS_RES 后发送
        if req.tx_res_received_at.is_none() {
            warn!(
                trade_no = %trade_no,
                source = "side_effect_worker",
                "Result ACK skipped: tx_res not received"
            );
            return Ok(());
        }

        if req.transaction_time.is_none() {
            warn!(
                trade_no = %trade_no,
                source = "side_effect_worker",
                "Transaction time is NULL; cannot send result ACK"
            );
            return Ok(());
        }

        // 标记结果确认尝试
        info!(trade_no = %trade_no, source = "side_effect_worker", "Marking result ACK as attempted");
        wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_result_ack_attempted(
            &self.pool,
            &trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        info!(trade_no = %trade_no, source = "side_effect_worker", "Result ACK marked as attempted successfully");

        // 获取backend_api
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        // 发送TxRes ACK
        match backend_api
            .trans_event_ack(
                &wallet_transport_backend::request::api_wallet::transaction::TransEventAckReq::new(
                    &trade_no,
                    wallet_transport_backend::request::api_wallet::transaction::TransType::Col,
                    wallet_transport_backend::request::api_wallet::transaction::TransAckType::TxRes,
                ),
            )
            .await
        {
            Ok(_) => {
                info!(trade_no = %trade_no, "TxRes ACK sent successfully");
                // 成功路径：原子标记结果确认已发送 + 标记归集订单已完成
                wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_result_ack_confirmed_and_mark_chain_finished(
                        &self.pool,
                        &trade_no,
                    ).await
                    .map_err(|e| {
                        error!(trade_no = %trade_no, error = %e, "Failed to mark result ACK confirmed and collect finished");
                        ServiceError::Database(e.into())
                    })?;

                // 直接调用 try_advance 进行点对点唤醒
                self.advancer.try_advance(&trade_no).await;
            }
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, "Failed to send TxRes ACK");
                // 失败路径：只保留 attempted 状态，让 Scanner 重试
                return Err(e.into());
            }
        }

        Ok(())
    }

    /// 处理发送手续费结果确认 ACK
    async fn process_tx_fee_res_ack(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "side_effect_worker", "Processing Tx Fee Res ACK command");

        // 获取交易信息
        let req = self.get_collect_entity(&trade_no).await?;

        // 幂等保护：检查是否已发送手续费结果确认 ACK
        if req.tx_fee_res_ack_sent_at.is_some() {
            info!(trade_no = %trade_no, source = "side_effect_worker", "Tx Fee Res ACK already sent, skipping");
            return Ok(());
        }

        // 获取backend_api
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        // 发送手续费结果确认 ACK
        match backend_api
            .trans_event_ack(
                &wallet_transport_backend::request::api_wallet::transaction::TransEventAckReq::new(
                    &trade_no,
                    wallet_transport_backend::request::api_wallet::transaction::TransType::Col,
                    wallet_transport_backend::request::api_wallet::transaction::TransAckType::TxFeeRes,
                ),
            )
            .await
        {
            Ok(_) => {
                info!(trade_no = %trade_no, "Tx Fee Res ACK sent successfully");
                // 成功路径：标记手续费结果确认 ACK 已发送
                let affected = wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_tx_fee_res_ack_sent(
                        &self.pool,
                        &trade_no,
                    ).await
                    .map_err(|e| {
                        error!(trade_no = %trade_no, error = %e, "Failed to mark Tx Fee Res ACK sent");
                        ServiceError::Database(e.into())
                    })?;
                if affected == 0 {
                    warn!(trade_no = %trade_no, "Tx Fee Res ACK marked 0 rows (trade_no missing or already marked)");
                }

                // 直接调用 try_advance 进行点对点唤醒
                self.advancer.try_advance(&trade_no).await;
            }
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, "Failed to send Tx Fee Res ACK");
                // 失败路径：让 Scanner 重试
                return Err(e.into());
            }
        }

        Ok(())
    }

    /// 处理上传服务费记录
    async fn process_upload_service_fee(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "side_effect_worker", "Processing UploadServiceFee command");

        // 获取交易信息
        let req = self.get_collect_entity(&trade_no).await?;

        // 幂等保护：检查服务费是否已上传
        // invariant: uploaded_at.is_some() => attempted_at.is_some()
        if req.service_fee_uploaded_at.is_some() {
            info!(trade_no = %trade_no, source = "side_effect_worker", "Service fee already uploaded, skipping");
            return Ok(());
        }

        // 解析出款地址
        let exec_from_addr = self.resolve_withdraw_from_addr(&req).await?;
        info!(trade_no = %trade_no, exec_from_addr = %exec_from_addr, source = "side_effect_worker", "Resolved withdrawal address");

        // 查询主币信息
        let main_coin = ApiChainTransDomain::main_coin(&req.chain_code).await?;
        info!(trade_no = %trade_no, source = "side_effect_worker", "Retrieved main coin information");

        // 解析链代码
        let chain_code: ChainCode = req.chain_code.as_str().try_into()?;
        let (fee_symbol, fee_token_key, fee_decimals) = self
            .resolve_fee_estimation_coin_info(&req.chain_code, &req.token_addr, &main_coin)
            .await?;

        let mut estimated_fee_str: Option<String> = None;
        let mut reestimated_due_to_non_positive_stored_fee = false;

        // 使用 transaction_fee 事实；若现有事实非正数，则视为不可靠并强制重估
        let fee = if req.transaction_fee.is_empty() {
            let fee_str = self
                .estimate_fee(
                    &req.from_addr,
                    &req.to_addr,
                    &req.value,
                    chain_code,
                    &fee_symbol,
                    &main_coin.symbol,
                    fee_token_key.clone(),
                    fee_decimals,
                )
                .await?;
            estimated_fee_str = Some(fee_str.clone());
            let fee = conversion::decimal_from_str(&fee_str)?;
            info!(trade_no = %trade_no, fee = %fee_str, source = "side_effect_worker", "Estimated fee successfully (transaction_fee was empty)");
            fee
        } else {
            let stored_fee = conversion::decimal_from_str(&req.transaction_fee)?;
            if stored_fee <= rust_decimal::Decimal::ZERO {
                reestimated_due_to_non_positive_stored_fee = true;
                warn!(
                    trade_no = %trade_no,
                    transaction_fee = %req.transaction_fee,
                    reason_code = "non_positive_transaction_fee_fact",
                    source = "side_effect_worker",
                    "Non-positive transaction_fee fact detected, re-estimating before service fee upload"
                );
                let fee_str = self
                    .estimate_fee(
                        &req.from_addr,
                        &req.to_addr,
                        &req.value,
                        chain_code,
                        &fee_symbol,
                        &main_coin.symbol,
                        fee_token_key.clone(),
                        fee_decimals,
                    )
                    .await?;
                estimated_fee_str = Some(fee_str.clone());
                let fee = conversion::decimal_from_str(&fee_str)?;
                info!(
                    trade_no = %trade_no,
                    fee = %fee_str,
                    source = "side_effect_worker",
                    "Re-estimated fee successfully after non-positive transaction_fee fact"
                );
                fee
            } else {
                info!(trade_no = %trade_no, fee = %req.transaction_fee, source = "side_effect_worker", "Using existing transaction_fee fact");
                stored_fee
            }
        };

        // 计算需要补充的手续费
        let mut fee_to_upload = if let Some(f) = fee.to_f64() { f } else { 0.0 };
        if chain_code == ChainCode::Ethereum || chain_code == ChainCode::BnbSmartChain {
            fee_to_upload = fee_to_upload * 2.0;
            info!(trade_no = %trade_no, source = "side_effect_worker", "Doubling fee for Ethereum/BSC network: {}", fee_to_upload);
        }

        if fee_to_upload <= 0.0 {
            warn!(
                trade_no = %trade_no,
                transaction_fee = %req.transaction_fee,
                estimated_fee = ?estimated_fee_str,
                fee_to_upload,
                reestimated_due_to_non_positive_stored_fee,
                reason_code = "non_positive_service_fee",
                source = "side_effect_worker",
                "Computed non-positive service fee; skipping upload and clearing need_service_fee for local self-heal"
            );

            let cleared_rows = wallet_database::repositories::api_wallet::collect::ApiCollectRepo::clear_need_service_fee(
                &self.pool,
                &trade_no,
            )
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;

            if cleared_rows == 0 {
                warn!(
                    trade_no = %trade_no,
                    rows_affected = %cleared_rows,
                    source = "side_effect_worker",
                    "clear_need_service_fee affected 0 rows during non-positive service fee self-heal"
                );
            } else {
                info!(
                    trade_no = %trade_no,
                    rows_affected = %cleared_rows,
                    source = "side_effect_worker",
                    "Cleared need_service_fee after non-positive service fee self-heal"
                );
            }

            self.advancer.try_advance(&trade_no).await;
            return Ok(());
        }

        // 标记服务费上传尝试
        info!(trade_no = %trade_no, source = "side_effect_worker", "Marking service fee as attempted");
        wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_service_fee_attempted(
            &self.pool,
            &trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        info!(trade_no = %trade_no, source = "side_effect_worker", "Service fee marked as attempted successfully");

        // 获取backend_api
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        // 构建服务费上传请求
        let upload_req = ServiceFeeUploadReq::new(
            &trade_no,
            &req.chain_code,
            &main_coin.symbol,
            "",
            &exec_from_addr,
            &req.from_addr,
            fee_to_upload,
        );

        // 上传服务费记录
        info!(trade_no = %trade_no, source = "side_effect_worker", "Uploading service fee record");
        backend_api.upload_service_fee_record(&upload_req).await?;
        info!(trade_no = %trade_no, source = "side_effect_worker", "Service fee record uploaded successfully");

        // 标记服务费已上传
        info!(trade_no = %trade_no, source = "side_effect_worker", "Marking service fee as uploaded");
        wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_service_fee_uploaded(
            &self.pool,
            &trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        info!(trade_no = %trade_no, source = "side_effect_worker", "Service fee marked as uploaded successfully");

        // 直接调用 try_advance 进行点对点唤醒
        self.advancer.try_advance(&trade_no).await;

        Ok(())
    }

    /// 处理上传交易执行回执
    async fn process_tx_exec_receipt(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "side_effect_worker", "Processing UploadTxExecReceipt command");

        // 获取交易信息
        let req = self.get_collect_entity(&trade_no).await?;

        if !Self::has_tx_exec_receipt_fact(&req) {
            info!(
                trade_no = %trade_no,
                source = "side_effect_worker",
                last_broadcast_at_present = %req.last_broadcast_at.is_some(),
                transaction_time_present = %req.transaction_time.is_some(),
                err_code_present = %req.err_code.is_some(),
                "TxExecReceipt still pending, skip upload"
            );
            return Ok(());
        }

        // 幂等保护：检查是否已上传执行回执
        if req.tx_exec_receipt_uploaded_at.is_some() {
            info!(trade_no = %trade_no, source = "side_effect_worker", "TxExecReceipt already uploaded, skipping");
            return Ok(());
        }

        // 标记执行回执上传尝试
        info!(trade_no = %trade_no, source = "side_effect_worker", "Marking TxExecReceipt as attempted");
        wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_tx_exec_receipt_attempted(
            &self.pool,
            &trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        info!(trade_no = %trade_no, source = "side_effect_worker", "TxExecReceipt marked as attempted successfully");

        // 获取backend_api
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        // 构建交易执行回执上传请求
        let upload_payload = Self::build_tx_exec_receipt_payload(&req, &trade_no).await?;
        info!(
            trade_no = %trade_no,
            tx_hash = %req.tx_hash.as_deref().unwrap_or_default(),
            report_to_addr = %req.to_addr,
            upload_payload = ?upload_payload,
            source = "side_effect_worker",
            "Built TxExecReceipt upload payload"
        );

        let tx_hash_missing =
            req.tx_hash.as_deref().map(str::trim).map(str::is_empty).unwrap_or(true);
        if upload_payload.is_success() && tx_hash_missing {
            error!(
                trade_no = %trade_no,
                source = "side_effect_worker",
                block_reason = "blocked_by_missing_tx_hash",
                last_broadcast_at_present = %req.last_broadcast_at.is_some(),
                transaction_time_present = %req.transaction_time.is_some(),
                tx_hash_is_none = %req.tx_hash.is_none(),
                tx_hash_is_empty = %req.tx_hash.as_deref().map(str::trim).map(str::is_empty).unwrap_or(false),
                need_service_fee = ?req.need_service_fee,
                "Skip UploadTxExecReceipt: blocked_by_missing_tx_hash (success payload requires non-empty tx_hash)"
            );
            return Err(ServiceError::Parameter(
                "success tx_exec_receipt requires non-empty tx_hash".to_string(),
            ));
        }

        // 上传交易执行回执
        match backend_api.upload_tx_exec_receipt(&upload_payload).await {
            Ok(_) => {
                info!(trade_no = %trade_no, "TxExecReceipt uploaded successfully");
                // 成功路径：标记执行回执已上传
                wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_tx_exec_receipt_uploaded(
                        &self.pool,
                        &trade_no,
                    ).await
                    .map_err(|e| {
                        error!(trade_no = %trade_no, error = %e, "Failed to mark TxExecReceipt uploaded");
                        ServiceError::Database(e.into())
                    })?;

                // 标记交易终态：所有必要的副作用已完成
                // 仅在“无成功证据且存在失败证据”时收口，避免链上已成功时误收口失败终态。
                if upload_payload.is_fail()
                    && req.transaction_time.is_none()
                    && req.err_code.is_some()
                {
                    info!(trade_no = %trade_no, source = "side_effect_worker", "Marking collect as finished");
                    wallet_database::repositories::api_wallet::collect::ApiCollectRepo::mark_chain_finished(
                        &self.pool,
                        &trade_no
                    ).await
                    .map_err(|e| {
                        error!(trade_no = %trade_no, error = %e, "Failed to mark collect as finished");
                        ServiceError::Database(e.into())
                    })?;
                    info!(trade_no = %trade_no, source = "side_effect_worker", "Collect marked as finished successfully");
                }
                // 直接调用 try_advance 进行点对点唤醒
                self.advancer.try_advance(&trade_no).await;
            }
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, "Failed to upload TxExecReceipt");
                // 失败路径：只保留 attempted 状态，让 Scanner 重试
                return Err(e.into());
            }
        }

        Ok(())
    }

    fn has_tx_exec_receipt_fact(
        req: &wallet_database::entities::api_collect::ApiCollectEntity,
    ) -> bool {
        req.transaction_time.is_some() || req.err_code.is_some()
    }

    fn select_fee_estimation_coin_info(
        token_addr: &AssetTokenKey,
        main_coin: &ApiCoinEntity,
        token_coin: Option<&ApiCoinEntity>,
    ) -> Result<(String, AssetTokenKey, u8), ServiceError> {
        if token_addr.is_contract() {
            let coin = token_coin.ok_or_else(|| {
                ServiceError::Parameter(format!(
                    "token coin not found for service fee estimation: token_addr={}",
                    token_addr.as_db_str()
                ))
            })?;
            Ok((coin.symbol.clone(), coin.token_address.clone(), coin.decimals))
        } else {
            Ok((main_coin.symbol.clone(), AssetTokenKey::Native, main_coin.decimals))
        }
    }

    async fn resolve_fee_estimation_coin_info(
        &self,
        chain_code: &str,
        token_addr: &AssetTokenKey,
        main_coin: &ApiCoinEntity,
    ) -> Result<(String, AssetTokenKey, u8), ServiceError> {
        if token_addr.is_contract() {
            let token_coin =
                ApiCoinDomain::get_coin_by_token_key_exact(chain_code, token_addr.clone()).await?;
            Self::select_fee_estimation_coin_info(token_addr, main_coin, Some(&token_coin))
        } else {
            Self::select_fee_estimation_coin_info(token_addr, main_coin, None)
        }
    }

    /// 估算手续费
    async fn estimate_fee(
        &self,
        from: &str,
        to: &str,
        value: &str,
        chain_code: ChainCode,
        symbol: &str,
        main_symbol: &str,
        token_key: AssetTokenKey,
        decimals: u8,
    ) -> Result<String, ServiceError> {
        info!(from=%from, to=%to, value=%value, chain_code=%chain_code.to_string(), symbol=%symbol, main_symbol=%main_symbol, token_address=%token_key.as_db_str(), source = "side_effect_worker", "Estimating transaction fee");

        let adapter = crate::domain::api_wallet::adapter_factory::ApiChainAdapterFactory::get_transaction_adapter(&chain_code.to_string()).await?;
        info!(chain_code=%chain_code.to_string(), source = "side_effect_worker", "Retrieved transaction adapter");

        let mut params = ApiBaseTransferReq::new(from, to, value, &chain_code.to_string());
        params.with_token(token_key.to_chain_token_option(), decimals, symbol);
        info!(chain_code=%chain_code.to_string(), source = "side_effect_worker", "Built transfer parameters");

        let fee = adapter.estimate_fee(params, main_symbol).await?;
        info!(chain_code=%chain_code.to_string(), source = "side_effect_worker", "Received fee estimate from adapter");

        // 解析手续费结果
        let amount = match chain_code {
            ChainCode::Tron => {
                let res: crate::response_vo::TronFeeDetails =
                    wallet_utils::serde_func::serde_from_str(&fee)?;
                res.estimate_fee.amount.to_string()
            }
            ChainCode::Bitcoin => todo!(),
            ChainCode::Solana => {
                let res: crate::response_vo::CommonFeeDetails =
                    wallet_utils::serde_func::serde_from_str(&fee)?;
                res.estimate_fee.amount.to_string()
            }
            ChainCode::Ethereum => {
                let res: crate::response_vo::FeeDetailsVo<crate::response_vo::EthereumFeeDetails> =
                    wallet_utils::serde_func::serde_from_str(&fee)?;
                let mut amount: f64 = 0.0;
                for it in res.data {
                    amount = amount + it.estimate_fee.amount;
                }
                amount.to_string()
            }
            ChainCode::BnbSmartChain => {
                let res: crate::response_vo::FeeDetailsVo<crate::response_vo::EthereumFeeDetails> =
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
        info!(chain_code=%chain_code.to_string(), amount=%amount, source = "side_effect_worker", "Parsed fee estimate successfully");

        Ok(amount)
    }

    /// 构建交易执行回执上传请求
    async fn build_tx_exec_receipt_payload(
        req: &wallet_database::entities::api_collect::ApiCollectEntity,
        trade_no: &str,
    ) -> Result<
        wallet_transport_backend::request::api_wallet::transaction::TxExecReceiptUploadReq,
        ServiceError,
    > {
        if !Self::has_tx_exec_receipt_fact(req) {
            return Err(ServiceError::Parameter(
                "tx_exec_receipt upload requires confirmed success or failure facts".to_string(),
            ));
        }

        // 构建状态
        let upload_status = if req.transaction_time.is_some() {
            wallet_transport_backend::request::api_wallet::transaction::TransStatus::Success
        } else if req.err_code.is_some() {
            wallet_transport_backend::request::api_wallet::transaction::TransStatus::Fail
        } else {
            wallet_transport_backend::request::api_wallet::transaction::TransStatus::Fail
        };

        let tx_hash_missing =
            req.tx_hash.as_deref().map(str::trim).map(str::is_empty).unwrap_or(true);
        if req.transaction_time.is_some() && tx_hash_missing {
            error!(
                trade_no = %trade_no,
                source = "side_effect_worker",
                transaction_time_present = %req.transaction_time.is_some(),
                last_broadcast_at_present = %req.last_broadcast_at.is_some(),
                tx_hash_is_none = %req.tx_hash.is_none(),
                tx_hash_is_empty = %req.tx_hash.as_deref().map(str::trim).map(str::is_empty).unwrap_or(false),
                err_code_present = %req.err_code.is_some(),
                "Inconsistent collect execution facts: execution evidence exists but tx_hash is missing"
            );
        }

        // 构建备注
        let remark = if matches!(
            upload_status,
            wallet_transport_backend::request::api_wallet::transaction::TransStatus::Success
        ) || req.err_msg.as_deref().unwrap_or("").is_empty()
        {
            ""
        } else {
            req.err_msg.as_deref().unwrap_or("")
        };

        // 构建请求
        let payload =
            wallet_transport_backend::request::api_wallet::transaction::TxExecReceiptUploadReq::new(
                Some(&req.from_addr),
                Some(&req.to_addr),
                trade_no,
                wallet_transport_backend::request::api_wallet::transaction::TransType::Col,
                req.tx_hash.as_deref(),
                upload_status,
                remark,
            );

        Ok(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::SideEffectWorker;
    use chrono::Utc;
    use wallet_database::entities::{
        api_coin::ApiCoinEntity,
        api_collect::{ApiCollectEntity, ApiCollectStatus, ErrCode},
        asset_token_key::AssetTokenKey,
    };

    fn make_coin(symbol: &str, token_address: AssetTokenKey, decimals: u8) -> ApiCoinEntity {
        ApiCoinEntity {
            id: 1,
            name: symbol.to_string(),
            symbol: symbol.to_string(),
            chain_code: "eth".to_string(),
            token_address,
            price: "0".to_string(),
            protocol: None,
            decimals,
            is_default: 1,
            is_popular: 0,
            is_custom: 0,
            status: 1,
            created_at: Utc::now(),
            updated_at: None,
        }
    }

    fn base_collect() -> ApiCollectEntity {
        ApiCollectEntity {
            id: 1,
            name: "n".to_string(),
            uid: "u".to_string(),
            from_addr: "from".to_string(),
            to_addr: "to".to_string(),
            value: "0".to_string(),
            validate: "v".to_string(),
            chain_code: "eth".to_string(),
            token_addr: AssetTokenKey::Native,
            symbol: "USDT".to_string(),
            trade_no: "C_SIDE_EFFECT_TEST".to_string(),
            trade_type: 2,
            risk_addr: 0,
            status: ApiCollectStatus::Init,
            nonce: 0,
            tx_hash: Some("h".to_string()),
            transaction_fee: "0".to_string(),
            transaction_time: None,
            block_height: Some("0".to_string()),
            notes: Some(String::new()),
            post_tx_count: 0,
            post_confirm_tx_count: 0,
            err_code: None,
            err_msg: Some(String::new()),
            order_ack_sent_at: Some(Utc::now()),
            raw_tx: Some("{}".to_string()),
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

    #[test]
    fn select_fee_estimation_coin_info_uses_token_decimals_for_contract_token() {
        let main_coin = make_coin("ETH", AssetTokenKey::Native, 18);
        let token_coin = make_coin(
            "USDT",
            AssetTokenKey::Contract("0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string()),
            6,
        );

        let (symbol, token_key, decimals) = SideEffectWorker::select_fee_estimation_coin_info(
            &AssetTokenKey::Contract("0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string()),
            &main_coin,
            Some(&token_coin),
        )
        .expect("contract token should resolve");

        assert_eq!(symbol, "USDT");
        assert_eq!(token_key, token_coin.token_address);
        assert_eq!(decimals, 6);
    }

    #[test]
    fn select_fee_estimation_coin_info_uses_main_coin_for_native_token() {
        let main_coin = make_coin("ETH", AssetTokenKey::Native, 18);
        let token_coin = make_coin(
            "USDT",
            AssetTokenKey::Contract("0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string()),
            6,
        );

        let (symbol, token_key, decimals) = SideEffectWorker::select_fee_estimation_coin_info(
            &AssetTokenKey::Native,
            &main_coin,
            Some(&token_coin),
        )
        .expect("native token should resolve");

        assert_eq!(symbol, "ETH");
        assert_eq!(token_key, AssetTokenKey::Native);
        assert_eq!(decimals, 18);
    }

    #[test]
    fn select_fee_estimation_coin_info_errors_when_contract_token_is_missing() {
        let main_coin = make_coin("ETH", AssetTokenKey::Native, 18);

        let err = SideEffectWorker::select_fee_estimation_coin_info(
            &AssetTokenKey::Contract("0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string()),
            &main_coin,
            None,
        )
        .expect_err("missing contract token should fail");

        assert!(err.to_string().contains("token coin not found"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn build_tx_exec_receipt_payload_marks_confirmed_success() {
        let mut c = base_collect();
        c.transaction_time = Some(Utc::now());
        c.last_broadcast_at = Some(Utc::now());
        c.tx_exec_receipt_uploaded_at = None;

        let payload = SideEffectWorker::build_tx_exec_receipt_payload(&c, &c.trade_no)
            .await
            .expect("confirmed success should build payload");

        assert!(payload.is_success());
    }

    #[tokio::test]
    async fn build_tx_exec_receipt_payload_marks_failure_fact() {
        let mut c = base_collect();
        c.transaction_time = None;
        c.err_code = Some(ErrCode::UnknownError);
        c.tx_exec_receipt_uploaded_at = None;

        let payload = SideEffectWorker::build_tx_exec_receipt_payload(&c, &c.trade_no)
            .await
            .expect("failure fact should build payload");

        assert!(payload.is_fail());
    }

    #[tokio::test]
    async fn build_tx_exec_receipt_payload_rejects_pending_facts() {
        let mut c = base_collect();
        c.transaction_time = None;
        c.err_code = None;
        c.last_broadcast_at = Some(Utc::now());
        c.tx_exec_receipt_uploaded_at = None;

        let err = SideEffectWorker::build_tx_exec_receipt_payload(&c, &c.trade_no)
            .await
            .expect_err("pending facts should be rejected");

        assert!(err.to_string().contains("confirmed success or failure facts"));
    }
}
