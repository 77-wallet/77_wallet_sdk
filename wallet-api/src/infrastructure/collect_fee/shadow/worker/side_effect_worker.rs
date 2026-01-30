// collect_fee/shadow/worker/side_effect_worker.rs

use tracing::{error, info, warn};
use wallet_database::{CollectDbPool, CoreDbPool, repositories::api_wallet::fee::ApiFeeRepo};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

/// SideEffectCommand 命令
#[derive(Debug, Clone)]
pub enum SideEffectCommand {
    /// 发送交易 ACK
    SendOrderAck(String),
    /// 发送交易结果 ACK
    SendResultAck(String),
    /// 上传交易执行回执
    UploadTxExecReceipt(String),
}

impl SideEffectCommand {
    /// 从 SideEffectCommand 生成对应的 RunningKey
    pub fn to_running_key(
        &self,
    ) -> crate::infrastructure::collect_fee::shadow::dispatcher::RunningKey {
        match self {
            SideEffectCommand::SendOrderAck(trade_no) => {
                crate::infrastructure::collect_fee::shadow::dispatcher::RunningKey::SendTxAck(trade_no.clone())
            }
            SideEffectCommand::SendResultAck(trade_no) => {
                crate::infrastructure::collect_fee::shadow::dispatcher::RunningKey::SendTxResAck(trade_no.clone())
            }
            SideEffectCommand::UploadTxExecReceipt(trade_no) => {
                crate::infrastructure::collect_fee::shadow::dispatcher::RunningKey::UploadTxExecReceipt(trade_no.clone())
            }
        }
    }

    /// 获取交易单号
    pub fn get_trade_no(&self) -> &str {
        match self {
            SideEffectCommand::SendOrderAck(trade_no) => trade_no,
            SideEffectCommand::SendResultAck(trade_no) => trade_no,
            SideEffectCommand::UploadTxExecReceipt(trade_no) => trade_no,
        }
    }
}

/// SideEffectWorker
///
/// 负责处理副作用操作：
/// - 发送交易 ACK
/// - 发送交易结果 ACK
/// - 上传交易执行回执
pub struct SideEffectWorker {
    pool: CollectDbPool,
    core_pool: CoreDbPool,
}

impl SideEffectWorker {
    pub fn new(pool: CollectDbPool, core_pool: CoreDbPool) -> Self {
        Self { pool, core_pool }
    }

    /// 处理命令
    pub async fn handle(&self, command: SideEffectCommand) -> Result<(), anyhow::Error> {
        match command {
            SideEffectCommand::SendOrderAck(trade_no) => self.send_order_ack(&trade_no).await,
            SideEffectCommand::SendResultAck(trade_no) => self.send_tx_res_ack(&trade_no).await,
            SideEffectCommand::UploadTxExecReceipt(trade_no) => {
                self.upload_tx_exec_receipt(&trade_no).await
            }
        }
    }

    /// 发送交易 ACK
    ///
    /// SideEffect: send_tx_ack
    /// Requires:
    /// - raw_tx IS NOT NULL
    /// - tx_ack_sent_at IS NULL
    async fn send_order_ack(&self, trade_no: &str) -> Result<(), anyhow::Error> {
        info!(trade_no = %trade_no, "Sending order ACK");

        // 强制读取事实，确保副作用基于最新 DB 状态（防止幻读）
        let fee = ApiFeeRepo::get_api_fee_by_trade_no(&self.pool, trade_no).await?;

        if fee.tx_ack_sent_at.is_some() {
            warn!(trade_no = %trade_no, "Tx ACK skipped: already sent");
            return Ok(());
        }

        // 标记交易 ACK 尝试
        info!(trade_no = %trade_no, "Marking tx ACK as attempted");
        ApiFeeRepo::mark_tx_ack_attempted(&self.pool, trade_no).await?;
        info!(trade_no = %trade_no, "Tx ACK marked as attempted successfully");

        // 发送交易 ACK 逻辑
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let trans_event_req =
            TransEventAckReq::new(&fee.trade_no, TransType::ColFee, TransAckType::Tx);

        match backend.trans_event_ack(&trans_event_req).await {
            Ok(_) => {
                info!(trade_no = %trade_no, "Tx ACK sent successfully");
                // 成功路径：标记交易 ACK 已发送
                if let Err(e) = ApiFeeRepo::set_tx_ack_sent(&self.pool, trade_no).await {
                    error!(trade_no = %trade_no, error = %e, "Failed to mark tx ACK sent");
                    return Err(e.into());
                }
            }
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, "Failed to send tx ACK");
                // 失败路径：只保留 attempted 状态，让 Scanner 重试
                return Err(e.into());
            }
        }

        info!(trade_no = %trade_no, "Tx ACK sent successfully");

        Ok(())
    }

    /// 发送交易结果 ACK
    ///
    /// SideEffect: send_tx_res_ack
    /// Requires:
    /// - tx_hash IS NOT NULL
    /// - tx_res_ack_sent_at IS NULL
    async fn send_tx_res_ack(&self, trade_no: &str) -> Result<(), anyhow::Error> {
        info!(trade_no = %trade_no, "Sending tx res ACK");

        // 强制读取事实，确保副作用基于最新 DB 状态（防止幻读）
        let fee = ApiFeeRepo::get_api_fee_by_trade_no(&self.pool, trade_no).await?;

        // 检查是否允许发送结果 ACK
        // - tx_hash 必须已存在
        // - 尚未发送过结果 ACK
        if fee.tx_hash.is_none() {
            warn!(trade_no = %trade_no, "Tx res ACK skipped: tx_hash not exists");
            return Ok(());
        }

        if fee.tx_res_ack_sent_at.is_some() {
            warn!(trade_no = %trade_no, "Tx res ACK skipped: already sent");
            return Ok(());
        }

        // 标记交易结果 ACK 尝试
        info!(trade_no = %trade_no, "Marking tx res ACK as attempted");
        ApiFeeRepo::mark_tx_res_ack_attempted(&self.pool, trade_no).await?;
        info!(trade_no = %trade_no, "Tx res ACK marked as attempted successfully");

        // 发送交易结果 ACK 逻辑
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let trans_event_req =
            TransEventAckReq::new(&fee.trade_no, TransType::ColFee, TransAckType::TxRes);

        match backend.trans_event_ack(&trans_event_req).await {
            Ok(_) => {
                info!(trade_no = %trade_no, "Tx res ACK sent successfully");
                // 成功路径：标记交易结果 ACK 已发送
                if let Err(e) = ApiFeeRepo::set_tx_res_ack_sent(&self.pool, trade_no).await {
                    error!(trade_no = %trade_no, error = %e, "Failed to mark tx res ACK sent");
                    return Err(e.into());
                }
                if let Err(e) = ApiFeeRepo::mark_chain_finished(&self.pool, trade_no).await {
                    error!(trade_no = %trade_no, error = %e, "Failed to mark fee as finished");
                    return Err(e.into());
                }
            }
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, "Failed to send tx res ACK");
                // 失败路径：只保留 attempted 状态，让 Scanner 重试
                return Err(e.into());
            }
        }

        info!(trade_no = %trade_no, "Tx res ACK sent successfully");

        Ok(())
    }

    /// 上传交易执行回执
    ///
    /// SideEffect: upload_tx_exec_receipt
    /// Requires:
    /// - transaction_time IS NOT NULL
    /// - tx_exec_receipt_uploaded_at IS NULL
    async fn upload_tx_exec_receipt(&self, trade_no: &str) -> Result<(), anyhow::Error> {
        info!(trade_no = %trade_no, "Uploading tx exec receipt");

        // 强制读取事实，确保副作用基于最新 DB 状态（防止幻读）
        let fee = ApiFeeRepo::get_api_fee_by_trade_no(&self.pool, trade_no).await?;

        if fee.tx_exec_receipt_uploaded_at.is_some() {
            warn!(trade_no = %trade_no, "Tx exec receipt upload skipped: already uploaded");
            return Ok(());
        }

        // 标记交易执行回执上传尝试
        info!(trade_no = %trade_no, "Marking tx exec receipt as attempted");
        ApiFeeRepo::mark_tx_exec_receipt_attempted(&self.pool, trade_no).await?;
        info!(trade_no = %trade_no, "Tx exec receipt marked as attempted successfully");

        // 获取backend_api
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        // 构建交易执行回执上传请求
        let upload_payload = self.build_tx_exec_receipt_payload(&fee, trade_no).await?;
        info!(trade_no = %trade_no, "Built tx exec receipt upload payload");

        // 上传交易执行回执
        match backend.upload_tx_exec_receipt(&upload_payload).await {
            Ok(_) => {
                info!(trade_no = %trade_no, "Tx exec receipt uploaded successfully");
                // 成功路径：标记交易执行回执已上传
                if let Err(e) =
                    ApiFeeRepo::mark_tx_exec_receipt_uploaded(&self.pool, trade_no).await
                {
                    error!(trade_no = %trade_no, error = %e, "Failed to mark tx exec receipt uploaded");
                    return Err(e.into());
                }

                // 标记交易终态：所有必要的副作用已完成
                if upload_payload.is_fail() {
                    info!(trade_no = %trade_no, source = "side_effect_worker", "Marking fee as finished");
                    if let Err(e) = ApiFeeRepo::mark_chain_finished(&self.pool, trade_no).await {
                        error!(trade_no = %trade_no, error = %e, "Failed to mark fee as finished");
                        return Err(e.into());
                    }
                    info!(trade_no = %trade_no, source = "side_effect_worker", "Fee marked as finished successfully");
                }
            }
            Err(e) => {
                error!(trade_no = %trade_no, error = %e, "Failed to upload tx exec receipt");
                // 失败路径：只保留 attempted 状态，让 Scanner 重试
                return Err(e.into());
            }
        }

        Ok(())
    }

    /// 构建交易执行回执上传请求
    async fn build_tx_exec_receipt_payload(
        &self,
        fee: &wallet_database::entities::api_fee::ApiFeeEntity,
        trade_no: &str,
    ) -> Result<
        wallet_transport_backend::request::api_wallet::transaction::TxExecReceiptUploadReq,
        anyhow::Error,
    > {
        // 构建状态
        let upload_status = if fee.last_broadcast_at.is_some() {
            // if fee.status == wallet_database::entities::api_fee::ApiFeeStatus::Success {
            wallet_transport_backend::request::api_wallet::transaction::TransStatus::Success
        } else {
            wallet_transport_backend::request::api_wallet::transaction::TransStatus::Fail
        };

        // 构建备注
        let remark = if fee.err_msg.is_empty() { "" } else { &fee.err_msg };

        // 构建请求
        let payload =
            wallet_transport_backend::request::api_wallet::transaction::TxExecReceiptUploadReq::new(
                Some(&fee.from_addr),
                Some(&fee.to_addr),
                trade_no,
                wallet_transport_backend::request::api_wallet::transaction::TransType::ColFee,
                fee.tx_hash.as_deref(),
                upload_status,
                remark,
            );

        Ok(payload)
    }
}
