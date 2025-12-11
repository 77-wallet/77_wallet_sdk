use crate::infrastructure::collect_fee::command::ProcessFeeTxConfirmReportCommand;
use chrono::TimeDelta;
use std::sync::Arc;
use tokio::{
    sync::{broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    entities::api_fee::{ApiFeeEntity, ApiFeeStatus},
    repositories::api_wallet::fee::ApiFeeRepo,
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

pub(super) struct ProcessFeeTxConfirmReport {
    pool: Arc<sqlx::SqlitePool>,
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessFeeTxConfirmReportCommand>,
}

impl ProcessFeeTxConfirmReport {
    pub(super) fn new(
        pool: Arc<sqlx::SqlitePool>,
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessFeeTxConfirmReportCommand>,
    ) -> Self {
        Self { pool, shutdown_rx, report_rx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!("starting process fee tx confirm report -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process fee tx confirm report -------------------------------");
                    break;
                }
                report_msg = self.report_rx.recv() => {
                    match report_msg {
                        Some(cmd) => {
                            match cmd {
                                ProcessFeeTxConfirmReportCommand::Tx(trade_no) => {
                                    self.process_fee_single_tx_confirm_report_by_trade_no(&trade_no).await;
                                    iv.reset();
                                }
                            }
                        }
                        None => {}
                    }
                }
                _ = iv.tick() => {
                    self.process_fee_tx_confirm_report().await
                }
            }
        }
        tracing::info!("closing process fee tx confirm report ------------------------------- end");
    }

    async fn process_fee_single_tx_confirm_report_by_trade_no(&self, trade_no: &str) {
        tracing::info!(trade_no=%trade_no, "[手续费归集确认] 根据交易编号处理单个手续费交易确认报告");
        let res = ApiFeeRepo::get_api_fee_by_trade_no(&self.pool, &trade_no).await;
        match res {
            Ok(fee) => {
                tracing::info!(trade_no=%trade_no, "[手续费归集确认] 找到待处理的手续费交易确认报告");
                self.process_fee_single_tx_confirm_report(fee).await;
            }
            Err(err) => {
                tracing::warn!(trade_no=%trade_no, "[手续费归集确认] 获取手续费交易确认报告失败: {}", err);
            }
        }
    }

    async fn process_fee_tx_confirm_report(&mut self) {
        tracing::info!("[手续费归集确认] 批量处理手续费交易确认报告");
        let res = ApiFeeRepo::page_api_fee_with_status(
            &self.pool,
            0,
            1000,
            &[ApiFeeStatus::Failure, ApiFeeStatus::Success],
        )
        .await;
        match res {
            Ok((_, transfer_fees)) => {
                tracing::info!(
                    "[手续费归集确认] 找到 {} 条待处理的手续费交易确认报告",
                    transfer_fees.len()
                );
                for req in transfer_fees {
                    self.process_fee_single_tx_confirm_report(req).await
                }
            }
            Err(err) => {
                tracing::warn!("[手续费归集确认] 获取手续费交易确认报告列表失败: {}", err);
            }
        }
    }

    async fn process_fee_single_tx_confirm_report(&self, req: ApiFeeEntity) {
        tracing::info!(trade_no=%req.trade_no,hash=%req.tx_hash,status=%req.status, "[手续费归集确认] 处理单个手续费交易确认报告");
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(req.post_confirm_tx_count as i64) {
            tracing::warn!(trade_no=%req.trade_no,
                "[手续费归集确认] 手续费交易确认报告处理超时，post_confirm_tx_count设置过长"
            );
            return;
        }
        if req.status == ApiFeeStatus::SendingTxFailed {
            tracing::warn!(trade_no=%req.trade_no, "[手续费归集确认] 手续费交易确认报告状态错误");
            return;
        };
        if !(req.status == ApiFeeStatus::Success || req.status == ApiFeeStatus::Failure) {
            tracing::warn!(trade_no=%req.trade_no, "[手续费归集确认] 手续费交易确认报告状态错误: {}", req.status);
            return;
        }
        tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 调用后端API发送交易确认报告");
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend_api
            .trans_event_ack(&TransEventAckReq::new(
                &req.trade_no,
                TransType::ColFee,
                TransAckType::TxRes,
            ))
            .await
        {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 交易确认报告发送成功");
                self.handle_confirm_report_success(req).await;
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[手续费归集确认] 交易确认报告发送失败: {}", err);
                self.handle_confirm_report_failed(req, err).await;
            }
        }
    }

    async fn handle_confirm_report_success(&self, req: ApiFeeEntity) {
        tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 处理交易确认报告发送成功");
        let next_status = if req.status == ApiFeeStatus::Success {
            tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 交易成功，更新状态为ConfirmSuccessReport");
            ApiFeeStatus::ConfirmSuccessReport
        } else {
            tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 交易失败，更新状态为ConfirmFailureReport");
            ApiFeeStatus::ConfirmFailureReport
        };

        let res = ApiFeeRepo::update_api_fee_next_status(
            &self.pool,
            &req.trade_no,
            req.status,
            next_status,
        )
        .await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 交易确认报告状态更新成功");
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "[手续费归集确认] 交易确认报告状态更新失败: {}", err);
            }
        }
    }

    async fn handle_confirm_report_failed(
        &self,
        req: ApiFeeEntity,
        err: wallet_transport_backend::Error,
    ) {
        tracing::error!(trade_no=%req.trade_no, "[手续费归集确认] 处理交易确认报告发送失败: {}", err);
        tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 更新手续费交易确认报告重试次数");
        let res =
            ApiFeeRepo::update_api_fee_post_confirm_tx_count(&self.pool, &req.trade_no, req.status)
                .await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 手续费交易确认报告重试次数更新成功");
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "[手续费归集确认] 手续费交易确认报告重试次数更新失败: {}", err);
            }
        }
    }
}
