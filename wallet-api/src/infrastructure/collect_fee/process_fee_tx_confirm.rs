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
        let res = ApiFeeRepo::get_api_fee_by_trade_no(&self.pool, &trade_no).await;
        match res {
            Ok(fee) => {
                self.process_fee_single_tx_confirm_report(fee).await;
            }
            Err(err) => {
                tracing::warn!(trade_no=%trade_no, "failed to get fee: {}", err);
            }
        }
    }

    async fn process_fee_tx_confirm_report(&mut self) {
        let res = ApiFeeRepo::page_api_fee_with_status(
            &self.pool,
            0,
            1000,
            &[ApiFeeStatus::Failure, ApiFeeStatus::Success],
        )
        .await;
        match res {
            Ok((_, transfer_fees)) => {
                for req in transfer_fees {
                    self.process_fee_single_tx_confirm_report(req).await
                }
            }
            Err(err) => {
                tracing::warn!("failed to get transfer_fees: {}", err);
            }
        }
    }

    async fn process_fee_single_tx_confirm_report(&self, req: ApiFeeEntity) {
        tracing::info!(trade_no=%req.trade_no,hash=%req.tx_hash,status=%req.status, "process_fee_single_tx_confirm_report ---------------------------------4");
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(req.post_confirm_tx_count as i64) {
            tracing::warn!(trade_no=%req.trade_no,
                "process_fee_single_tx_confirm_report timeout post confirm_tx_count is too long"
            );
            return;
        }
        if req.status == ApiFeeStatus::SendingTxFailed {
            tracing::warn!(trade_no=%req.trade_no, "process_fee_single_tx_confirm_report status is wrong");
            return;
        };
        if !(req.status == ApiFeeStatus::Success || req.status == ApiFeeStatus::Failure) {
            tracing::warn!(trade_no=%req.trade_no, "process_fee_single_tx_confirm_report status is wrong {}", req.status);
            return;
        }
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend_api
            .trans_event_ack(&TransEventAckReq::new(
                &req.trade_no,
                TransType::ColFee,
                TransAckType::TxRes,
            ))
            .await
        {
            Ok(_) => self.handle_confirm_report_success(req).await,
            Err(err) => self.handle_confirm_report_failed(req, err).await,
        }
    }

    async fn handle_confirm_report_success(&self, req: ApiFeeEntity) {
        let next_status = if req.status == ApiFeeStatus::Success {
            ApiFeeStatus::ConfirmSuccessReport
        } else {
            ApiFeeStatus::ConfirmFailureReport
        };
        tracing::info!(trade_no=%req.trade_no, "process_fee_single_tx_confirm_report success");
        let res = ApiFeeRepo::update_api_fee_next_status(
            &self.pool,
            &req.trade_no,
            req.status,
            next_status,
            "fee trans event ack",
        )
        .await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "process_fee_single_tx_confirm_report success");
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "process_fee_single_tx_confirm_report failed: {}", err);
            }
        }
    }

    async fn handle_confirm_report_failed(
        &self,
        req: ApiFeeEntity,
        err: wallet_transport_backend::Error,
    ) {
        tracing::error!(trade_no=%req.trade_no, "failed to process fee tx confirm report: {}", err);
        let res =
            ApiFeeRepo::update_api_fee_post_confirm_tx_count(&self.pool, &req.trade_no, req.status)
                .await;
        match res {
            Ok(_) => (),
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "failed to process fee tx confirm report: {}", err);
            }
        }
    }
}
