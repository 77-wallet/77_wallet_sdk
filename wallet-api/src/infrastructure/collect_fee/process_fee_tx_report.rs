use crate::infrastructure::collect_fee::command::ProcessFeeTxReportCommand;
use chrono::TimeDelta;
use serde_json::json;
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
    TransStatus, TransType, TxExecReceiptUploadReq,
};

pub(super) struct ProcessFeeTxReport {
    pool: Arc<sqlx::SqlitePool>,
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessFeeTxReportCommand>,
}

impl ProcessFeeTxReport {
    pub(super) fn new(
        pool: Arc<sqlx::SqlitePool>,
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessFeeTxReportCommand>,
    ) -> Self {
        Self { pool, shutdown_rx, report_rx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!("starting process fee tx report -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process fee tx report -------------------------------");
                    break;
                }
                report_msg = self.report_rx.recv() => {
                    if let Some(cmd) = report_msg {
                        match cmd {
                            ProcessFeeTxReportCommand::Tx(trade_no) => {
                                self.process_fee_single_tx_report_by_trade_no(&trade_no).await;
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    self.process_fee_tx_report().await
                }
            }
        }
        tracing::info!("closing process fee tx report ------------------------------- end");
    }

    async fn process_fee_single_tx_report_by_trade_no(&self, trade_no: &str) {
        tracing::info!(trade_no=%trade_no, "[手续费归集报告] 根据交易编号处理单个手续费交易报告");
        let res = ApiFeeRepo::get_api_fee_by_trade_no_status(
            &self.pool,
            &trade_no,
            &[ApiFeeStatus::SendingTx, ApiFeeStatus::SendingTxFailed],
        )
        .await;
        match res {
            Ok(api_fee) => {
                tracing::info!(trade_no=%trade_no, "[手续费归集报告] 找到待处理的手续费交易报告");
                // 直接调用时不检查重试时间
                self.process_fee_single_tx_report(api_fee, false).await;
            }
            Err(err) => {
                tracing::warn!(trade_no=%trade_no, "[手续费归集报告] 获取手续费交易报告失败: {}", err);
            }
        }
    }

    async fn process_fee_tx_report(&mut self) {
        tracing::info!("[手续费归集报告] 批量处理手续费交易报告");
        let res = ApiFeeRepo::page_api_fee_with_status(
            &self.pool,
            0,
            1000,
            &[ApiFeeStatus::SendingTx, ApiFeeStatus::SendingTxFailed],
        )
        .await;
        match res {
            Ok((_, transfer_fees)) => {
                tracing::info!(
                    "[手续费归集报告] 找到 {} 条待处理的手续费交易报告",
                    transfer_fees.len()
                );
                for req in transfer_fees {
                    // 定时检查时需要检查重试时间
                    self.process_fee_single_tx_report(req, true).await
                }
            }
            Err(err) => {
                tracing::warn!("[手续费归集报告] 获取手续费交易报告列表失败: {}", err);
            }
        }
    }

    async fn process_fee_single_tx_report(&self, req: ApiFeeEntity, check_retry_time: bool) {
        tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 处理单个手续费交易报告");

        // 只有在需要检查重试时间时才执行检查
        if check_retry_time {
            // 判断超时时间
            let now = chrono::Utc::now();
            let timeout = now - req.updated_at.unwrap();
            tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 当前时间: {}, 上次更新时间: {}, 超时时间: {}, 当前重试次数: {}", 
                        now, req.updated_at.unwrap(), timeout, req.post_tx_count);

            if timeout < TimeDelta::seconds(1 << req.post_tx_count as i64) {
                tracing::warn!(trade_no=%req.trade_no, "[手续费归集报告] 未到重试时间，跳过本次处理");
                return;
            }
        } else {
            tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 直接调用，跳过重试时间检查");
        }
        let (status, remark) = if req.status == ApiFeeStatus::SendingTxFailed {
            tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 交易发送失败，准备上传失败报告");
            let msg = json!({
                "code": req.err_code,
                "msg": req.err_msg,
            });
            let s = msg.to_string();
            tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 失败报告内容: {}", s);
            (TransStatus::Fail, s)
        } else {
            tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 交易发送成功，准备上传成功报告");
            (TransStatus::Success, "".to_string())
        };

        tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 调用后端API上传交易执行报告");
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend_api
            .upload_tx_exec_receipt(&TxExecReceiptUploadReq::new(
                &req.trade_no,
                TransType::ColFee,
                &req.tx_hash,
                status,
                remark.as_str(),
            ))
            .await
        {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 交易执行报告上传成功");
                self.handle_report_success(req).await;
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[手续费归集报告] 交易执行报告上传失败: {}", err);
                self.handle_report_failed(req, err).await;
            }
        }
    }

    async fn handle_report_success(&self, req: ApiFeeEntity) {
        tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 处理交易执行报告上传成功");
        let (next_status, notes) = if req.status == ApiFeeStatus::SendingTxFailed {
            tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 交易发送失败报告上传成功，更新状态为SendingTxFailedReport");
            (
                ApiFeeStatus::SendingTxFailedReport,
                "upload server ok for transfer fee send tx failed",
            )
        } else {
            tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 交易发送成功报告上传成功，更新状态为SendingTxReport");
            (ApiFeeStatus::SendingTxReport, "upload server ok for transfer fee success")
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
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 交易报告状态更新成功: {}", notes);
            }
            Err(_) => {
                tracing::error!(trade_no=%req.trade_no, "[手续费归集报告] 交易报告状态更新失败");
            }
        }
    }

    async fn handle_report_failed(&self, req: ApiFeeEntity, err: wallet_transport_backend::Error) {
        tracing::error!(trade_no=%req.trade_no, "[手续费归集报告] 处理交易执行报告上传失败: {}", err);
        tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 更新手续费交易报告重试次数");
        let res =
            ApiFeeRepo::update_api_fee_post_tx_count(&self.pool, &req.trade_no, req.status).await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 手续费交易报告重试次数更新成功");
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "[手续费归集报告] 手续费交易报告重试次数更新失败: {:?}", err);
            }
        }
    }
}
