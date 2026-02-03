// withdraw/shadow/worker/side_effect_worker.rs
use std::sync::Arc;

use tracing::{error, info};
use wallet_database::{
    CollectDbPool, CoreDbPool, repositories::api_wallet::withdraw::ApiWithdrawRepo,
};

use crate::{error::service::ServiceError, infrastructure::withdraw::shadow::ShadowScanner};

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
pub struct SideEffectWorker {
    pool: CollectDbPool,
    core_pool: CoreDbPool,
    /// ShadowScanner 引用，用于直接调用 try_advance
    scanner: Arc<ShadowScanner>,
}

impl SideEffectWorker {
    pub fn new(pool: CollectDbPool, core_pool: CoreDbPool, scanner: Arc<ShadowScanner>) -> Self {
        Self { pool, core_pool, scanner }
    }

    /// 处理命令
    pub async fn handle(&self, command: super::SideEffectCommand) -> Result<(), ServiceError> {
        match command {
            super::SideEffectCommand::SendTxAck(trade_no) => {
                self.process_send_tx_ack(trade_no).await
            }
            super::SideEffectCommand::SendTxResAck(trade_no) => {
                self.process_send_tx_res_ack(trade_no).await
            }
            super::SideEffectCommand::UploadTxExecReceipt(trade_no) => {
                self.process_upload_tx_exec_receipt(trade_no).await
            }
        }
    }

    /// 发送交易 ACK
    async fn process_send_tx_ack(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "side_effect_worker", "Processing SendTxAck command");

        // 1. 从数据库中获取提币交易信息
        let withdraw = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &self.pool,
            &trade_no,
            wallet_database::entities::api_trade_type::ApiTradeType::Withdraw,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        // 2. 事实校验：SendTxAck 只能处理 tx_ack_sent_at 为空的交易
        if withdraw.tx_ack_sent_at.is_some() {
            info!(trade_no = %trade_no, source = "side_effect_worker", "tx_ack_sent_at already exists, skipping SendTxAck");
            return Ok(());
        }

        // 3. 发送交易 ACK
        // TODO: 实现发送交易 ACK 的逻辑
        // 这里需要调用外部服务发送 ACK，例如通过 HTTP 或 MQTT

        // 4. 更新数据库状态
        let rows_affected = ApiWithdrawRepo::mark_tx_ack_sent(&self.pool, &trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;

        // 5. 显式处理幂等情况：ACK 已被其他并发执行
        if rows_affected == 0 {
            info!(trade_no = %trade_no, source = "side_effect_worker", "mark_tx_ack_sent skipped: ACK already sent (idempotent hit)");
        } else {
            // 6. 直接调用 try_advance 进行点对点唤醒
            self.scanner.try_advance(&trade_no).await;
        }

        Ok(())
    }

    /// 发送交易结果 ACK
    async fn process_send_tx_res_ack(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "side_effect_worker", "Processing SendTxResAck command");

        // 1. 从数据库中获取提币交易信息
        let withdraw = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &self.pool,
            &trade_no,
            wallet_database::entities::api_trade_type::ApiTradeType::Withdraw,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        // 2. 事实校验：SendTxResAck 只能处理 transaction_time 存在且 tx_res_ack_sent_at 为空的交易
        if withdraw.transaction_time.is_none() || withdraw.tx_res_ack_sent_at.is_some() {
            info!(trade_no = %trade_no, source = "side_effect_worker", "transaction_time empty or tx_res_ack_sent_at exists, skipping SendTxResAck");
            return Ok(());
        }

        // 3. 发送交易结果 ACK
        // TODO: 实现发送交易结果 ACK 的逻辑
        // 这里需要调用外部服务发送 ACK，例如通过 HTTP 或 MQTT

        // 4. 更新数据库状态
        let rows_affected = ApiWithdrawRepo::mark_tx_res_ack_sent(&self.pool, &trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;

        // 5. 显式处理幂等情况：ACK 已被其他并发执行
        if rows_affected == 0 {
            info!(trade_no = %trade_no, source = "side_effect_worker", "mark_tx_res_ack_sent skipped: ACK already sent (idempotent hit)");
        } else {
            // 6. 直接调用 try_advance 进行点对点唤醒
            self.scanner.try_advance(&trade_no).await;
        }

        Ok(())
    }

    /// 上传交易执行回执
    async fn process_upload_tx_exec_receipt(&self, trade_no: String) -> Result<(), ServiceError> {
        info!(trade_no = %trade_no, source = "side_effect_worker", "Processing UploadTxExecReceipt command");

        // 1. 从数据库中获取提币交易信息
        let withdraw = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &self.pool,
            &trade_no,
            wallet_database::entities::api_trade_type::ApiTradeType::Withdraw,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        // 2. 事实校验：UploadTxExecReceipt 只能处理有交易哈希的交易
        if withdraw.tx_hash.is_empty() {
            info!(trade_no = %trade_no, source = "side_effect_worker", "tx_hash empty, skipping UploadTxExecReceipt");
            return Ok(());
        }

        // 3. 上传交易执行回执
        // TODO: 实现上传交易执行回执的逻辑
        // 这里需要调用外部服务上传回执，例如通过 HTTP

        // 4. 更新数据库状态
        let rows_affected = ApiWithdrawRepo::mark_tx_exec_receipt_uploaded(&self.pool, &trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;

        // 5. 显式处理幂等情况：回执已被其他并发上传
        if rows_affected == 0 {
            info!(trade_no = %trade_no, source = "side_effect_worker", "mark_tx_exec_receipt_uploaded skipped: receipt already uploaded (idempotent hit)");
        } else {
            // 6. 直接调用 try_advance 进行点对点唤醒
            self.scanner.try_advance(&trade_no).await;
        }

        Ok(())
    }
}
