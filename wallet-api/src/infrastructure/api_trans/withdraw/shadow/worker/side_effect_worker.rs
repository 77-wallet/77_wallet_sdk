// withdraw/shadow/worker/side_effect_worker.rs
use std::sync::Arc;

use tracing::{error, info, warn};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool,
    entities::api_trade_type::ApiTradeType,
    repositories::api_wallet::{
        resource_delegation::ApiResourceDelegationRepo, withdraw::ApiWithdrawRepo,
    },
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransStatus, TransType, TxExecReceiptUploadReq,
};

use crate::{
    error::service::ServiceError, infrastructure::api_trans::withdraw::shadow::ShadowScanner,
};

enum ResourceGateReleaseOutcome<'a> {
    Success(&'a str),
    FailureBypass(&'a str),
}

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
    pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
    /// ShadowScanner 引用，用于直接调用 try_advance
    scanner: Arc<ShadowScanner>,
}

impl SideEffectWorker {
    pub fn new(
        pool: ApiTransactionDbPool,
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
            super::SideEffectCommand::SendResourceResultAck(trade_no) => trade_no,
            super::SideEffectCommand::SendResourceTaskAck(trade_no) => trade_no,
            super::SideEffectCommand::UploadResourceTxExecReceipt(trade_no) => trade_no,
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
                super::SideEffectCommand::SendResourceResultAck(trade_no) => {
                    self_clone.process_resource_result_ack(trade_no).await
                }
                super::SideEffectCommand::SendResourceTaskAck(trade_no) => {
                    self_clone.process_resource_task_ack(trade_no).await
                }
                super::SideEffectCommand::UploadResourceTxExecReceipt(trade_no) => {
                    self_clone.process_resource_tx_exec_receipt(trade_no).await
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

        let Some(_tx_ack_guard) =
            crate::infrastructure::api_trans::withdraw::tx_ack_gate::try_acquire_tx_ack_gate(
                &trade_no,
            )
        else {
            info!(trade_no = %trade_no, source = "side_effect_worker", "Tx ACK skipped: already in flight");
            return Ok(());
        };

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

        // ✅ 强顺序屏障：TX_RES ACK 只能在已收到并持久化 AWM_ORDER_TRANS_RES 后发送
        if withdraw.tx_res_received_at.is_none() {
            info!(
                trade_no = %trade_no,
                source = "side_effect_worker",
                "Tx res ACK skipped: tx_res not received"
            );
            return Ok(());
        }

        if withdraw.tx_res_ack_sent_at.is_some() {
            if withdraw.finished_at.is_none() && withdraw.transaction_time.is_some() {
                // 兼容历史半完成事实：tx_res_ack 已写但 finished 未写（例如 kill -9）
                info!(
                    trade_no = %trade_no,
                    source = "side_effect_worker",
                    "Tx res ACK already sent but withdraw not finished; repairing finished_at"
                );
                ApiWithdrawRepo::mark_chain_finished(&self.pool, &trade_no)
                    .await
                    .map_err(|e| ServiceError::Database(e.into()))?;
                self.scanner.try_advance(&trade_no).await;
            } else if withdraw.transaction_time.is_none() {
                info!(
                    trade_no = %trade_no,
                    source = "side_effect_worker",
                    "Tx res ACK already sent but transaction_time is NULL; skip repairing finished_at"
                );
            }

            info!(
                trade_no = %trade_no,
                source = "side_effect_worker",
                "Tx res ACK skipped: already sent"
            );
            return Ok(());
        }

        if withdraw.transaction_time.is_none() {
            info!(
                trade_no = %trade_no,
                source = "side_effect_worker",
                "Transaction time is NULL; cannot send tx res ACK"
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

    async fn process_resource_result_ack(
        &self,
        resource_trade_no: String,
    ) -> Result<(), ServiceError> {
        let resource_task =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;

        if resource_task.result_ack_sent_at.is_some() || resource_task.result_received_at.is_none()
        {
            return Ok(());
        }

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        backend
            .trans_event_ack(&TransEventAckReq::new(
                &resource_trade_no,
                Self::resource_delegation_trans_type(&resource_task),
                TransAckType::TxRscRes,
            ))
            .await?;

        let affected =
            ApiResourceDelegationRepo::mark_result_ack_sent(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            warn!(resource_trade_no = %resource_trade_no, "Resource result ACK marked 0 rows");
        }

        self.project_resource_task_outcome_to_withdraw_gate(
            &resource_task,
            ResourceGateReleaseOutcome::Success("resource_delegation_success"),
        )
        .await?;
        Ok(())
    }

    async fn process_resource_task_ack(
        &self,
        resource_trade_no: String,
    ) -> Result<(), ServiceError> {
        let resource_task =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;

        if resource_task.task_ack_sent_at.is_some() {
            return Ok(());
        }

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        backend
            .trans_event_ack(&TransEventAckReq::new(
                &resource_trade_no,
                Self::resource_delegation_trans_type(&resource_task),
                TransAckType::Tx,
            ))
            .await?;

        let affected =
            ApiResourceDelegationRepo::mark_task_ack_sent(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            warn!(resource_trade_no = %resource_trade_no, "Resource task ACK marked 0 rows");
        }
        Ok(())
    }

    async fn process_resource_tx_exec_receipt(
        &self,
        resource_trade_no: String,
    ) -> Result<(), ServiceError> {
        let resource_task =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;

        if resource_task.tx_exec_receipt_uploaded_at.is_some() {
            return Ok(());
        }

        let payload = Self::build_resource_tx_exec_receipt_payload(&resource_task)?;
        let tx_hash_missing =
            resource_task.tx_hash.as_deref().map(str::trim).map(str::is_empty).unwrap_or(true);
        if payload.is_success() && tx_hash_missing {
            return Err(ServiceError::Parameter(
                "resource delegation success receipt requires non-empty tx_hash".to_string(),
            ));
        }

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        backend.upload_tx_exec_receipt(&payload).await?;
        let affected = ApiResourceDelegationRepo::mark_tx_exec_receipt_uploaded(
            &self.pool,
            &resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            warn!(resource_trade_no = %resource_trade_no, "Resource tx exec receipt marked 0 rows");
        }

        self.project_resource_task_outcome_to_withdraw_gate(
            &resource_task,
            ResourceGateReleaseOutcome::FailureBypass("resource_delegation_failed_bypass"),
        )
        .await?;
        Ok(())
    }

    async fn project_resource_task_outcome_to_withdraw_gate(
        &self,
        resource_task: &wallet_database::entities::api_resource_delegation::ApiResourceDelegationEntity,
        outcome: ResourceGateReleaseOutcome<'_>,
    ) -> Result<(), ServiceError> {
        let release_result = match outcome {
            ResourceGateReleaseOutcome::Success(release_result) => {
                if resource_task.err_code.is_some()
                    || !matches!(resource_task.tx_status.as_deref(), Some("success"))
                {
                    return Ok(());
                }
                release_result
            }
            ResourceGateReleaseOutcome::FailureBypass(release_result) => {
                let is_failure = resource_task.err_code.is_some()
                    || matches!(resource_task.tx_status.as_deref(), Some("fail"));
                if !is_failure {
                    return Ok(());
                }
                release_result
            }
        };

        if resource_task.origin_trade_type != Some(ApiTradeType::Withdraw as i64) {
            return Ok(());
        }
        let Some(origin_trade_no) = resource_task.origin_trade_no.as_deref() else {
            return Ok(());
        };

        let withdraw = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &self.pool,
            origin_trade_no,
            ApiTradeType::Withdraw,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        if withdraw.resource_gate_released_at.is_some() {
            self.scanner.try_advance(origin_trade_no).await;
            return Ok(());
        }

        ApiWithdrawRepo::mark_resource_released(&self.pool, origin_trade_no, release_result)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        self.scanner.try_advance(origin_trade_no).await;
        Ok(())
    }

    fn build_resource_tx_exec_receipt_payload(
        resource_task: &wallet_database::entities::api_resource_delegation::ApiResourceDelegationEntity,
    ) -> Result<TxExecReceiptUploadReq, ServiceError> {
        let trans_type = Self::resource_delegation_trans_type(resource_task);
        let status = if matches!(resource_task.tx_status.as_deref(), Some("success")) {
            TransStatus::Success
        } else if resource_task.err_code.is_some() {
            TransStatus::Fail
        } else {
            return Err(ServiceError::Parameter(
                "resource delegation receipt upload requires success tx_status or failure err_code"
                    .to_string(),
            ));
        };
        let remark = if matches!(status, TransStatus::Success) {
            ""
        } else {
            resource_task.err_msg.as_deref().unwrap_or("")
        };
        let mut payload = TxExecReceiptUploadReq::new(
            Some(&resource_task.owner_address),
            Some(&resource_task.receiver_address),
            &resource_task.resource_trade_no,
            trans_type,
            resource_task.tx_hash.as_deref(),
            status,
            remark,
        );
        if let Some(err_code) = resource_task.err_code.as_deref().filter(|s| !s.trim().is_empty()) {
            payload = payload.with_error_code(err_code);
        }
        Ok(payload)
    }

    fn resource_delegation_trans_type(
        resource_task: &wallet_database::entities::api_resource_delegation::ApiResourceDelegationEntity,
    ) -> TransType {
        match resource_task.origin_trade_type {
            Some(x) if x == ApiTradeType::Withdraw as i64 => TransType::WdRscDl,
            _ => TransType::ColRscDl,
        }
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

        // 幂等保护：检查是否已上传执行回执
        if withdraw.tx_exec_receipt_uploaded_at.is_some() {
            info!(trade_no = %trade_no, source = "side_effect_worker", "TxExecReceipt already uploaded, skipping");
            return Ok(());
        }

        // 构建交易执行回执上传请求
        let upload_payload = self.build_tx_exec_receipt_payload(&withdraw, &trade_no).await?;
        info!(trade_no = %trade_no, source = "side_effect_worker", "Built tx exec receipt upload payload");

        let tx_hash_missing =
            withdraw.tx_hash.as_deref().map(str::trim).map(str::is_empty).unwrap_or(true);
        if upload_payload.is_success() && tx_hash_missing {
            error!(
                trade_no = %trade_no,
                source = "side_effect_worker",
                block_reason = "blocked_by_missing_tx_hash",
                chain_success_at_present = %withdraw.chain_success_at.is_some(),
                chain_failed_at_present = %withdraw.chain_failed_at.is_some(),
                transaction_time_present = %withdraw.transaction_time.is_some(),
                last_broadcast_at_present = %withdraw.last_broadcast_at.is_some(),
                tx_hash_is_none = %withdraw.tx_hash.is_none(),
                tx_hash_is_empty = %withdraw.tx_hash.as_deref().map(str::trim).map(str::is_empty).unwrap_or(false),
                "Skip UploadTxExecReceipt: blocked_by_missing_tx_hash (success payload requires non-empty tx_hash)"
            );
            return Err(ServiceError::Parameter(
                "success tx_exec_receipt requires non-empty tx_hash".to_string(),
            ));
        }

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
                    // 仅在“无成功证据且存在失败证据”时收口，避免链上已成功时误收口失败终态。
                    if upload_payload.is_fail()
                        && withdraw.transaction_time.is_none()
                        && withdraw.chain_success_at.is_none()
                        && (withdraw.chain_failed_at.is_some() || withdraw.err_code.is_some())
                    {
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
            TransType, TxExecReceiptUploadReq,
        };

        let upload_status = tx_exec_receipt_upload_status(withdraw);

        let tx_hash_missing =
            withdraw.tx_hash.as_deref().map(str::trim).map(str::is_empty).unwrap_or(true);
        let has_success_execution_evidence = (withdraw.chain_success_at.is_some()
            || withdraw.transaction_time.is_some())
            && withdraw.chain_failed_at.is_none();
        if has_success_execution_evidence && tx_hash_missing {
            error!(
                trade_no = %trade_no,
                source = "side_effect_worker",
                chain_success_at_present = %withdraw.chain_success_at.is_some(),
                chain_failed_at_present = %withdraw.chain_failed_at.is_some(),
                transaction_time_present = %withdraw.transaction_time.is_some(),
                last_broadcast_at_present = %withdraw.last_broadcast_at.is_some(),
                tx_hash_is_none = %withdraw.tx_hash.is_none(),
                tx_hash_is_empty = %withdraw.tx_hash.as_deref().map(str::trim).map(str::is_empty).unwrap_or(false),
                err_code_present = %withdraw.err_code.is_some(),
                "Inconsistent withdraw execution facts: execution evidence exists but tx_hash is missing"
            );
        }

        // 构建备注
        let remark = if matches!(
            upload_status,
            wallet_transport_backend::request::api_wallet::transaction::TransStatus::Success
        ) {
            ""
        } else {
            withdraw.err_msg.as_deref().unwrap_or("")
        };

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

fn tx_exec_receipt_upload_status(
    withdraw: &wallet_database::entities::api_withdraw::ApiWithdrawEntity,
) -> wallet_transport_backend::request::api_wallet::transaction::TransStatus {
    if withdraw.chain_success_at.is_some() || withdraw.transaction_time.is_some() {
        wallet_transport_backend::request::api_wallet::transaction::TransStatus::Success
    } else if withdraw.chain_failed_at.is_some() || withdraw.err_code.is_some() {
        wallet_transport_backend::request::api_wallet::transaction::TransStatus::Fail
    } else {
        wallet_transport_backend::request::api_wallet::transaction::TransStatus::Fail
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use wallet_database::entities::{
        api_trade_type::ApiTradeType,
        api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus, WithdrawFailureStage},
        asset_token_key::AssetTokenKey,
    };

    fn base_withdraw(trade_no: &str) -> ApiWithdrawEntity {
        ApiWithdrawEntity {
            id: 1,
            name: "n".to_string(),
            uid: "u".to_string(),
            from_addr: "from".to_string(),
            to_addr: "to".to_string(),
            value: "0".to_string(),
            validate: "v".to_string(),
            chain_code: "tron".to_string(),
            token_addr: AssetTokenKey::Native,
            symbol: "s".to_string(),
            trade_no: trade_no.to_string(),
            trade_type: ApiTradeType::Withdraw,
            init_status: ApiWithdrawStatus::Init,
            status: ApiWithdrawStatus::Init,
            nonce: 0,
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
            resource_check_at: None,
            resource_gate_released_at: None,
            resource_gate_result: None,
            resource_block_reason: None,
            resource_dependency_trade_no: None,
            resource_dependency_type: None,
            tx_ack_sent_at: Some(Utc::now()),
            building_at: None,
            last_broadcast_at: None,
            broadcast_uncertain_since_at: None,
            broadcast_uncertain_retry_count: 0,
            broadcast_uncertain_last_checked_at: None,
            broadcast_uncertain_reconciled_at: None,
            broadcast_uncertain_rebroadcast_count: 0,
            tx_res_ack_sent_at: None,
            tx_res_received_at: None,
            tx_exec_receipt_uploaded_at: None,
            finished_at: None,
            audit_passed_at: Some(Utc::now()),
            audit_rejected_at: None,
            audit_reason: None,
            chain_success_at: None,
            chain_failed_at: None,
            failure_stage: Some(WithdrawFailureStage::Unknown),
            created_at: Utc::now(),
            updated_at: Some(Utc::now()),
        }
    }

    #[test]
    fn tx_exec_receipt_status_is_success_only_for_confirmed_success() {
        let mut withdraw = base_withdraw("W1");
        withdraw.transaction_time = Some(Utc::now());

        assert_eq!(
            tx_exec_receipt_upload_status(&withdraw),
            wallet_transport_backend::request::api_wallet::transaction::TransStatus::Success
        );
    }

    #[test]
    fn tx_exec_receipt_status_is_not_success_for_broadcast_only_pending() {
        let mut withdraw = base_withdraw("W2");
        withdraw.last_broadcast_at = Some(Utc::now());

        assert_eq!(
            tx_exec_receipt_upload_status(&withdraw),
            wallet_transport_backend::request::api_wallet::transaction::TransStatus::Fail
        );
    }

    #[test]
    fn tx_exec_receipt_status_is_fail_for_failure_facts() {
        let mut withdraw = base_withdraw("W3");
        withdraw.err_code = Some(wallet_database::entities::api_withdraw::ErrCode::UnknownError);

        assert_eq!(
            tx_exec_receipt_upload_status(&withdraw),
            wallet_transport_backend::request::api_wallet::transaction::TransStatus::Fail
        );
    }
}
