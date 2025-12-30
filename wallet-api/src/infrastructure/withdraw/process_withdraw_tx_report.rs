// process_withdraw_tx_report.rs
use crate::infrastructure::withdraw::command::ProcessWithdrawTxReportCommand;
use chrono::TimeDelta;
use serde_json::json;
use std::sync::Arc;
use tokio::{
    sync::{broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
    repositories::api_wallet::withdraw::ApiWithdrawRepo,
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::{
    Error,
    request::api_wallet::transaction::{TransStatus, TransType, TxExecReceiptUploadReq},
};

pub(super) struct ProcessWithdrawTxReport {
    pool: Arc<sqlx::SqlitePool>,
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessWithdrawTxReportCommand>,
}

impl ProcessWithdrawTxReport {
    pub(super) fn new(
        pool: Arc<sqlx::SqlitePool>,
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessWithdrawTxReportCommand>,
    ) -> Self {
        Self { pool, shutdown_rx, report_rx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!("starting process withdraw tx report -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process withdraw tx report -------------------------------");
                    break;
                }
                msg = self.report_rx.recv() => {
                    if let Some(cmd) = msg {
                        match cmd {
                            ProcessWithdrawTxReportCommand::Tx(trade_no) => {
                                self.process_withdraw_single_tx_report_by_trade_no(&trade_no).await;
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    self.process_withdraw_tx_report().await
                }
            }
        }
        tracing::info!("closing process withdraw tx report ------------------------------- end");
    }

    async fn process_withdraw_single_tx_report_by_trade_no(&mut self, trade_no: &str) {
        let res = ApiWithdrawRepo::get_api_withdraw_by_trade_no_status(
            &self.pool,
            &trade_no,
            &[ApiWithdrawStatus::SendingTx, ApiWithdrawStatus::SendingTxFailed],
        )
        .await;
        match res {
            Ok(api_withdraw) => self.process_withdraw_single_tx_report(api_withdraw).await,
            Err(err) => {
                tracing::warn!(trade_no=%trade_no, "process withdraw single tx report by id: {:?}", err);
            }
        }
    }

    async fn process_withdraw_tx_report(&mut self) {
        let res = ApiWithdrawRepo::list_api_withdraw_with_status(
            &self.pool,
            vec![ApiWithdrawStatus::SendingTx, ApiWithdrawStatus::SendingTxFailed],
            0,
            1000,
        )
        .await;
        match res {
            Ok(api_withdraws) => {
                for req in api_withdraws {
                    self.process_withdraw_single_tx_report(req).await
                }
            }
            Err(err) => {
                tracing::warn!("process withdraw tx report by id: {:?}", err);
            }
        }
    }

    async fn process_withdraw_single_tx_report(&self, req: ApiWithdrawEntity) {
        tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_withdraw_single_tx_report ---------------------------------4");
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(req.post_tx_count as i64) {
            tracing::warn!(trade_no=%req.trade_no,
                "process_withdraw_single_tx_report timed out, post_tx_count: {}, timeout: {}",
                req.post_tx_count,
                timeout
            );
            return;
        }
        // 转成服务需要的状态
        let (status, remark) = if req.status == ApiWithdrawStatus::SendingTxFailed {
            let msg = json!({
                "code": req.err_code,
                "msg": req.err_msg,
            });
            let s = msg.to_string();
            (TransStatus::Fail, s)
        } else {
            (TransStatus::Success, "".to_string())
        };
        let tx_exec_receipt_upload_req = TxExecReceiptUploadReq::new(
            None,
            None,
            &req.trade_no,
            TransType::Wd,
            &req.tx_hash,
            status,
            remark.as_str(),
        );
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend_api.upload_tx_exec_receipt(&tx_exec_receipt_upload_req).await {
            Ok(_) => {
                self.handle_report_success(req).await;
            }
            Err(err) => {
                self.handle_report_failed(req, err).await;
            }
        }
    }

    async fn handle_report_success(&self, req: ApiWithdrawEntity) {
        tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_withdraw_single_tx_report ok");
        let next_status = if req.status == ApiWithdrawStatus::SendingTxFailed {
            ApiWithdrawStatus::SendingTxFailedReport
        } else {
            ApiWithdrawStatus::SendingTxReport
        };
        let res = ApiWithdrawRepo::update_api_withdraw_next_status(
            &self.pool,
            &req.trade_no,
            req.status,
            next_status,
        )
        .await;
        match res {
            Ok(res) => {
                if (res != 1) {
                    tracing::warn!(trade_no=%req.trade_no, "failed to process withdraw tx confirm: {:?}", res);
                } else {
                    tracing::info!(trade_no=%req.trade_no, "upload tx exec receipt success ---");
                }
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "upload tx exec receipt error: {:?}", err);
            }
        }
    }

    async fn handle_report_failed(&self, req: ApiWithdrawEntity, err: Error) {
        tracing::error!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_withdraw_single_tx_report -----------{}", err);
        let res =
            ApiWithdrawRepo::update_api_fee_post_tx_count(&self.pool, &req.trade_no, req.status)
                .await;
        match res {
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "process withdraw tx report error: {:?}", err);
            }
        }
    }
}
