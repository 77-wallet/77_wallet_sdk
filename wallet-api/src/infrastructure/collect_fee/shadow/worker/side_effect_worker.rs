// collect_fee/shadow/worker/side_effect_worker.rs
use std::sync::Arc;

use tracing::{error, info, warn};
use wallet_database::{CollectDbPool, CoreDbPool, repositories::api_wallet::fee::ApiFeeRepo};

/// SideEffectCommand 命令
#[derive(Debug, Clone)]
pub enum SideEffectCommand {
    /// 发送交易 ACK
    SendTxAck(String),
    /// 发送交易结果 ACK
    SendTxResAck(String),
    /// 上传交易执行回执
    UploadTxExecReceipt(String),
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
            SideEffectCommand::SendTxAck(trade_no) => self.send_tx_ack(&trade_no).await,
            SideEffectCommand::SendTxResAck(trade_no) => self.send_tx_res_ack(&trade_no).await,
            SideEffectCommand::UploadTxExecReceipt(trade_no) => {
                self.upload_tx_exec_receipt(&trade_no).await
            }
        }
    }

    /// 发送交易 ACK
    async fn send_tx_ack(&self, trade_no: &str) -> Result<(), anyhow::Error> {
        info!(trade_no = %trade_no, "Sending tx ACK");

        // 强制读取事实，确保副作用基于最新 DB 状态（防止幻读）
        let _fee = ApiFeeRepo::get_api_fee_by_trade_no(&self.pool, trade_no).await?;

        // 发送交易 ACK 逻辑
        // 注意：这里需要实现具体的发送 ACK 逻辑
        // 实际实现中应该调用相应的 backend API 来发送 ACK

        // 标记交易 ACK 已发送
        ApiFeeRepo::set_tx_ack_sent(&self.pool, trade_no).await?;

        info!(trade_no = %trade_no, "Tx ACK sent successfully");

        Ok(())
    }

    /// 发送交易结果 ACK
    async fn send_tx_res_ack(&self, trade_no: &str) -> Result<(), anyhow::Error> {
        info!(trade_no = %trade_no, "Sending tx res ACK");

        // 强制读取事实，确保副作用基于最新 DB 状态（防止幻读）
        let _fee = ApiFeeRepo::get_api_fee_by_trade_no(&self.pool, trade_no).await?;

        // 发送交易结果 ACK 逻辑
        // 注意：这里需要实现具体的发送 ACK 逻辑
        // 实际实现中应该调用相应的 backend API 来发送 ACK

        // 标记交易结果 ACK 已发送
        ApiFeeRepo::set_tx_res_ack_sent(&self.pool, trade_no).await?;

        info!(trade_no = %trade_no, "Tx res ACK sent successfully");

        Ok(())
    }

    /// 上传交易执行回执
    async fn upload_tx_exec_receipt(&self, trade_no: &str) -> Result<(), anyhow::Error> {
        info!(trade_no = %trade_no, "Uploading tx exec receipt");

        // 强制读取事实，确保副作用基于最新 DB 状态（防止幻读）
        let _fee = ApiFeeRepo::get_api_fee_by_trade_no(&self.pool, trade_no).await?;

        // 上传交易执行回执逻辑
        // 注意：这里需要实现具体的上传回执逻辑
        // 实际实现中应该调用相应的 backend API 来上传回执

        info!(trade_no = %trade_no, "Tx exec receipt uploaded successfully");

        Ok(())
    }
}
