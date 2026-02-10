// withdraw/shadow/worker/side_effect_worker.rs
use std::sync::Arc;

use tracing::{error, info};
use wallet_database::{
    ApiWalletDbPool, CollectDbPool, repositories::api_wallet::withdraw::ApiWithdrawRepo,
};

use crate::{
    error::service::ServiceError, infrastructure::api_trans::withdraw::shadow::ShadowScanner,
};

/// SideEffectWorker
///
/// 负责处理外部依赖的副作用操作：
/// - 发送交易 ACK
/// - 发送交易结果 ACK
/// - 上传交易执行回执
///
/// SideEffectWorker design invariant:
///
/// - 所有副作用操作必须基于已确认的链事实
/// - 所有副作用操作必须有并发安全保障
/// - 所有副作用操作必须是幂等的
/// - 禁止修改链事实
#[derive(Clone)]
pub struct SideEffectWorker {
    pool: CollectDbPool,
    core_pool: ApiWalletDbPool,
    /// ShadowScanner 引用，用于直接调用 try_advance
    scanner: Arc<ShadowScanner>,
}

impl SideEffectWorker {
    pub fn new(
        pool: CollectDbPool,
        core_pool: ApiWalletDbPool,
        scanner: Arc<ShadowScanner>,
    ) -> Self {
        Self { pool, core_pool, scanner }
    }

    /// 处理命令
    pub async fn handle(&self, command: super::SideEffectCommand) -> Result<(), ServiceError> {
        // 提取 trade_no 用于日志
        let trade_no = match &command {
            super::SideEffectCommand::SendTxAck(trade_no) => trade_no,
            super::SideEffectCommand::SendTxResAck(trade_no) => trade_no,
            super::SideEffectCommand::UploadTxExecReceipt(trade_no) => trade_no,
        };

        let trade_no_clone = trade_no.to_string();
        let self_clone = self.clone();

        match tokio::time::timeout(std::time::Duration::from_secs(30), async move {
            match command {
                super::SideEffectCommand::SendTxAck(trade_no) => {
                    self_clone.process_send_tx_ack(trade_no).await
                }
                super::SideEffectCommand::SendTxResAck(trade_no) => {
                    self_clone.process_send_tx_res_ack(trade_no).await
                }
                super::SideEffectCommand::UploadTxExecReceipt(trade_no) => {
                    self_clone.process_upload_tx_exec_receipt(trade_no).await
                }
            }
        })
        .await
        {
            Ok(result) => result,
            Err(_) => {
                error!(trade_no = %trade_no_clone, source = "side_effect_worker", "SideEffectWorker timeout after 30 seconds");
                Err(ServiceError::Timeout)
            }
        }
    }

    /// 发送交易 ACK
    async fn process_send_tx_ack(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "side_effect_worker", "Processing SendTxAck command");

        // 强制读取事实，确保副作用基于最新 DB 状态（防止幻读）
        let withdraw = match ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &self.pool,
            &trade_no,
            wallet_database::entities::api_trade_type::ApiTradeType::Withdraw,
        )
        .await
        {
            Ok(withdraw) => withdraw,
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, source = "side_effect_worker", "Failed to get API withdraw by trade no");
                return Err(ServiceError::Database(e.into()));
            }
        };

        if withdraw.tx_ack_sent_at.is_some() {
            info!(trade_no = %trade_no, source = "side_effect_worker", "Tx ACK skipped: already sent");
            return Ok(());
        }

        // 发送交易 ACK 逻辑
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        use wallet_transport_backend::request::api_wallet::transaction::{
            TransAckType, TransEventAckReq, TransType,
        };
        let trans_event_req =
            TransEventAckReq::new(&withdraw.trade_no, TransType::Wd, TransAckType::Tx);

        match backend.trans_event_ack(&trans_event_req).await {
            Ok(_) => {
                info!(trade_no = %trade_no, source = "side_effect_worker", "Tx ACK sent successfully");
                // 成功路径：标记交易 ACK 已发送
                let rows_affected = ApiWithdrawRepo::mark_tx_ack_sent(&self.pool, &trade_no)
                    .await
                    .map_err(|e| ServiceError::Database(e.into()))?;

                // 显式处理幂等情况：ACK 已被其他并发执行
                if rows_affected == 0 {
                    info!(trade_no = %trade_no, source = "side_effect_worker", "mark_tx_ack_sent skipped: ACK already sent (idempotent hit)");
                } else {
                    // 直接调用 try_advance 进行点对点唤醒
                    self.scanner.try_advance(&trade_no).await;
                }
            }
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, source = "side_effect_worker", "Failed to send tx ACK");
                // 失败路径：让 Scanner 重试
            }
        }

        Ok(())
    }

    /// 发送交易结果 ACK
    async fn process_send_tx_res_ack(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "side_effect_worker", "Processing SendTxResAck command");

        // 强制读取事实，确保副作用基于最新 DB 状态（防止幻读）
        let withdraw = match ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &self.pool,
            &trade_no,
            wallet_database::entities::api_trade_type::ApiTradeType::Withdraw,
        )
        .await
        {
            Ok(withdraw) => withdraw,
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, source = "side_effect_worker", "Failed to get API withdraw by trade no");
                return Err(ServiceError::Database(e.into()));
            }
        };

        // 检查是否允许发送结果 ACK
        // - tx_hash 必须已存在
        // - 尚未发送过结果 ACK
        if withdraw.tx_hash.is_none() {
            info!(trade_no = %trade_no, source = "side_effect_worker", "Tx res ACK skipped: tx_hash not exists");
            return Ok(());
        }

        if withdraw.tx_res_ack_sent_at.is_some() {
            // 兼容历史半完成事实：tx_res_ack 已写但 finished 未写（例如 kill -9）
            if withdraw.finished_at.is_none() {
                if withdraw.transaction_time.is_none() {
                    info!(
                        trade_no = %trade_no,
                        source = "side_effect_worker",
                        "Tx res ACK already sent but transaction_time is NULL; skip repairing finished_at"
                    );
                    return Ok(());
                }
                info!(
                    trade_no = %trade_no,
                    source = "side_effect_worker",
                    "Tx res ACK already sent but withdraw not finished; repairing finished_at"
                );
                ApiWithdrawRepo::mark_chain_finished(&self.pool, &trade_no)
                    .await
                    .map_err(|e| ServiceError::Database(e.into()))?;
                self.scanner.try_advance(&trade_no).await;
            }

            info!(
                trade_no = %trade_no,
                source = "side_effect_worker",
                "Tx res ACK skipped: already sent"
            );
            return Ok(());
        }

        // 发送交易结果 ACK 逻辑
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        use wallet_transport_backend::request::api_wallet::transaction::{
            TransAckType, TransEventAckReq, TransType,
        };
        let trans_event_req =
            TransEventAckReq::new(&withdraw.trade_no, TransType::Wd, TransAckType::TxRes);

        match backend.trans_event_ack(&trans_event_req).await {
            Ok(_) => {
                info!(trade_no = %trade_no, source = "side_effect_worker", "Tx res ACK sent successfully");
                // 成功路径：标记交易结果 ACK 已发送
                let rows_affected =
                    ApiWithdrawRepo::mark_tx_res_ack_sent_and_chain_finished(&self.pool, &trade_no)
                        .await
                        .map_err(|e| ServiceError::Database(e.into()))?;

                // 显式处理幂等情况：ACK 已被其他并发执行
                if rows_affected == 0 {
                    info!(trade_no = %trade_no, source = "side_effect_worker", "mark_tx_res_ack_sent skipped: ACK already sent (idempotent hit)");
                } else {
                    // 直接调用 try_advance 进行点对点唤醒
                    self.scanner.try_advance(&trade_no).await;
                }
            }
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, source = "side_effect_worker", "Failed to send tx res ACK");
                // 失败路径：让 Scanner 重试
            }
        }

        Ok(())
    }

    /// 上传交易执行回执
    async fn process_upload_tx_exec_receipt(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "side_effect_worker", "Processing UploadTxExecReceipt command");

        // 强制读取事实，确保副作用基于最新 DB 状态（防止幻读）
        let withdraw = match ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &self.pool,
            &trade_no,
            wallet_database::entities::api_trade_type::ApiTradeType::Withdraw,
        )
        .await
        {
            Ok(withdraw) => withdraw,
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, source = "side_effect_worker", "Failed to get API withdraw by trade no");
                return Err(ServiceError::Database(e.into()));
            }
        };

        // 检查是否已经上传过执行回执
        // 注意：ApiWithdrawEntity 结构体中没有 tx_exec_receipt_uploaded_at 字段
        // 这里暂时跳过检查，直接尝试上传
        // 后续会通过数据库操作的幂等性来处理重复上传的情况

        // 构建交易执行回执上传请求
        let upload_payload = self.build_tx_exec_receipt_payload(&withdraw, &trade_no).await?;
        info!(trade_no = %trade_no, source = "side_effect_worker", "Built tx exec receipt upload payload");

        // 上传交易执行回执
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend.upload_tx_exec_receipt(&upload_payload).await {
            Ok(_) => {
                info!(trade_no = %trade_no, source = "side_effect_worker", "Tx exec receipt uploaded successfully");
                // 成功路径：标记交易执行回执已上传
                if let Err(e) =
                    ApiWithdrawRepo::mark_tx_exec_receipt_uploaded(&self.pool, &trade_no).await
                {
                    error!(trade_no = %trade_no, error = %e, source = "side_effect_worker", "Failed to mark tx exec receipt uploaded");
                } else {
                    // 标记交易终态：所有必要的副作用已完成
                    if upload_payload.is_fail() {
                        info!(trade_no = %trade_no, source = "side_effect_worker", "Marking withdraw as finished");
                        if let Err(e) =
                            ApiWithdrawRepo::mark_chain_finished(&self.pool, &trade_no).await
                        {
                            error!(trade_no = %trade_no, error = %e, source = "side_effect_worker", "Failed to mark withdraw as finished");
                        } else {
                            info!(trade_no = %trade_no, source = "side_effect_worker", "Withdraw marked as finished successfully");
                        }
                    }
                    // 直接调用 try_advance 进行点对点唤醒
                    self.scanner.try_advance(&trade_no).await;
                }
            }
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, source = "side_effect_worker", "Failed to upload tx exec receipt");
                // 失败路径：让 Scanner 重试
            }
        }

        Ok(())
    }

    /// 构建交易执行回执上传请求
    async fn build_tx_exec_receipt_payload(
        &self,
        withdraw: &wallet_database::entities::api_withdraw::ApiWithdrawEntity,
        trade_no: &str,
    ) -> Result<
        wallet_transport_backend::request::api_wallet::transaction::TxExecReceiptUploadReq,
        ServiceError,
    > {
        use wallet_transport_backend::request::api_wallet::transaction::{
            TransStatus, TransType, TxExecReceiptUploadReq,
        };

        // 构建状态
        let upload_status =
            if withdraw.tx_hash.is_some() { TransStatus::Success } else { TransStatus::Fail };

        // 构建备注
        let remark = withdraw.err_msg.as_deref().unwrap_or("");

        // 构建请求
        let payload = TxExecReceiptUploadReq::new(
            Some(&withdraw.from_addr),
            Some(&withdraw.to_addr),
            trade_no,
            TransType::Wd,
            withdraw.tx_hash.as_deref(),
            upload_status,
            remark,
        );

        Ok(payload)
    }
}
