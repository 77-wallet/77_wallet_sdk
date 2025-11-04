use crate::infrastructure::collect::command::ProcessCollectTxReportCommand;
use chrono::TimeDelta;
use std::sync::Arc;
use tokio::{
    sync::{broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::collect::ApiCollectRepo,
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::transaction::{
    TransStatus, TransType, TxExecReceiptUploadReq,
};

pub(super) struct ProcessCollectTxReport {
    pool: Arc<sqlx::SqlitePool>,
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessCollectTxReportCommand>,
}

impl ProcessCollectTxReport {
    pub(super) fn new(
        pool: Arc<sqlx::SqlitePool>,
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessCollectTxReportCommand>,
    ) -> Self {
        Self { pool, shutdown_rx, report_rx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!("starting process collect tx report -------------------------------");
        self.run_with_err().await;
        tracing::info!("closing process collect tx report ------------------------------- end");
    }

    async fn run_with_err(&mut self) {
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process collect tx report -------------------------------");
                    break;
                }
                report_msg = self.report_rx.recv() => {
                    if let Some(cmd) = report_msg {
                        match cmd {
                            ProcessCollectTxReportCommand::Tx(trade_no) => {
                                self.process_collect_single_tx_report_by_trade_no(&trade_no).await;
                                iv.reset();
                            }
                        }

                    }
                }
                _ = iv.tick() => {
                    self.process_collect_tx_report().await
                }
            }
        }
    }

    async fn process_collect_single_tx_report_by_trade_no(&self, trade_no: &str) {
        let res = ApiCollectRepo::get_api_collect_by_trade_no_status(
            &self.pool,
            &trade_no,
            &[ApiCollectStatus::SendingTx, ApiCollectStatus::SendingTxFailed],
        )
        .await;
        match res {
            Ok(res) => self.process_collect_single_tx_report(res).await,
            Err(_) => {
                tracing::warn!(trade_no=%trade_no, "failed to process collect tx report");
            }
        }
    }

    async fn process_collect_tx_report(&mut self) {
        let res = ApiCollectRepo::page_api_collect_with_status(
            &self.pool,
            0,
            1000,
            &[ApiCollectStatus::SendingTx, ApiCollectStatus::SendingTxFailed],
        )
        .await;
        match res {
            Ok((_, transfer_fees)) => {
                for req in transfer_fees {
                    self.process_collect_single_tx_report(req).await
                }
            }
            Err(err) => {
                tracing::warn!("failed to process collect tx report: {}", err);
            }
        }
    }

    async fn process_collect_single_tx_report(&self, req: ApiCollectEntity) {
        tracing::info!(trade_no=%req.trade_no, "process collect tx report -------------------------------");
        // 判断超时时间
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(1 << req.post_tx_count as i64) {
            tracing::warn!(trade_no=%req.trade_no, "process collect tx report timeout ---");
            return;
        }
        let status = if req.status == ApiCollectStatus::SendingTxFailed {
            TransStatus::Fail
        } else {
            TransStatus::Success
        };
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend_api
            .upload_tx_exec_receipt(&TxExecReceiptUploadReq::new(
                &req.trade_no,
                TransType::Col,
                &req.tx_hash,
                status,
                &req.notes,
            ))
            .await
        {
            Ok(_) => self.handle_report_success(req).await,
            Err(err) => self.handle_report_failed(req, err).await,
        }
    }

    async fn handle_report_success(&self, req: ApiCollectEntity) {
        let (next_status, notes) = if req.status == ApiCollectStatus::SendingTxFailed {
            (ApiCollectStatus::SendingTxFailedReport, "uploaded server ok for collect tx failed")
        } else {
            (ApiCollectStatus::SendingTxReport, "uploaded server ok for collect tx success")
        };
        let res = ApiCollectRepo::update_api_collect_next_status(
            &self.pool,
            &req.trade_no,
            req.status,
            next_status,
            notes,
        )
        .await;
        match res {
            Ok(_) => {
                tracing::info!("upload tx exec receipt success ---");
            }
            Err(err) => {
                tracing::error!("failed to process collect tx report: {}", err);
            }
        }
    }

    async fn handle_report_failed(
        &self,
        req: ApiCollectEntity,
        err: wallet_transport_backend::Error,
    ) {
        let res =
            ApiCollectRepo::update_api_collect_post_tx_count(&self.pool, &req.trade_no, req.status)
                .await;
        match res {
            Ok(_) => {}
            Err(err) => {
                tracing::error!("failed to process collect tx report: {}", err);
            }
        }
    }
}
