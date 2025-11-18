use crate::infrastructure::collect_fee::command::ProcessFeeTxReportCommand;
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
        let res = ApiFeeRepo::get_api_fee_by_trade_no_status(
            &self.pool,
            &trade_no,
            &[ApiFeeStatus::SendingTx, ApiFeeStatus::SendingTxFailed],
        )
        .await;
        match res {
            Ok(api_fee) => {
                self.process_fee_single_tx_report(api_fee).await;
            }
            Err(err) => {
                tracing::warn!("failed to process fee tx report: {}", err);
            }
        }
    }

    async fn process_fee_tx_report(&mut self) {
        let res = ApiFeeRepo::page_api_fee_with_status(
            &self.pool,
            0,
            1000,
            &[ApiFeeStatus::SendingTx, ApiFeeStatus::SendingTxFailed],
        )
        .await;
        match res {
            Ok((_, transfer_fees)) => {
                for req in transfer_fees {
                    self.process_fee_single_tx_report(req).await
                }
            }
            Err(err) => {
                tracing::warn!("failed to process fee tx report: {}", err);
            }
        }
    }

    async fn process_fee_single_tx_report(&self, req: ApiFeeEntity) {
        tracing::info!(trade_no=%req.trade_no, "process fee tx report -------------------------------");
        // 判断超时时间
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(1 << req.post_tx_count as i64) {
            tracing::warn!(trade_no=%req.trade_no, "process fee tx report timeout ---");
            return;
        }
        let status = if req.status == ApiFeeStatus::SendingTxFailed {
            TransStatus::Fail
        } else {
            TransStatus::Success
        };
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend_api
            .upload_tx_exec_receipt(&TxExecReceiptUploadReq::new(
                &req.trade_no,
                TransType::ColFee,
                &req.tx_hash,
                status,
                format!("code: {}, msg: {}", req.err_code, req.err_msg).as_str(),
            ))
            .await
        {
            Ok(_) => {
                self.handle_report_success(req).await;
            }
            Err(err) => {
                self.handle_report_failed(req, err).await;
            }
        }
    }

    async fn handle_report_success(&self, req: ApiFeeEntity) {
        let (next_status, notes) = if req.status == ApiFeeStatus::SendingTxFailed {
            (
                ApiFeeStatus::SendingTxFailedReport,
                "upload server ok for transfer fee send tx failed",
            )
        } else {
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
                tracing::info!("upload tx exec receipt success ---");
            }
            Err(_) => {
                tracing::error!("upload tx exec receipt failed");
            }
        }
    }

    async fn handle_report_failed(&self, req: ApiFeeEntity, err: wallet_transport_backend::Error) {
        let res =
            ApiFeeRepo::update_api_fee_post_tx_count(&self.pool, &req.trade_no, req.status).await;
        match res {
            Ok(_) => {}
            Err(err) => {
                tracing::warn!("process transfer fee tx report error: {:?}", err);
            }
        }
    }
}
