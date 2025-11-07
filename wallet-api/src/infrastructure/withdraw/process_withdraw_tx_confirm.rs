use crate::infrastructure::withdraw::command::ProcessWithdrawTxConfirmReportCommand;
use chrono::TimeDelta;
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
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

pub(super) struct ProcessWithdrawTxConfirmReport {
    pool: Arc<sqlx::SqlitePool>,
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessWithdrawTxConfirmReportCommand>,
}

impl ProcessWithdrawTxConfirmReport {
    pub(super) fn new(
        pool: Arc<sqlx::SqlitePool>,
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessWithdrawTxConfirmReportCommand>,
    ) -> Self {
        Self { pool, shutdown_rx, report_rx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!(
            "starting process withdraw tx confirm report -------------------------------"
        );
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process withdraw tx confirm report -------------------------------");
                    break;
                }
                msg = self.report_rx.recv() => {
                    if let Some(cmd) = msg {
                        match cmd {
                            ProcessWithdrawTxConfirmReportCommand::Tx(trade_no) => {
                                self.process_withdraw_single_tx_confirm_report_by_trade_no(&trade_no).await;
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                     self.process_withdraw_tx_confirm_report().await
                }
            }
        }
        tracing::info!(
            "closing process withdraw tx confirm report ------------------------------- end"
        );
    }

    async fn process_withdraw_single_tx_confirm_report_by_trade_no(&self, trade_no: &str) {
        let res = ApiWithdrawRepo::get_api_withdraw_by_trade_no_status(
            &self.pool,
            trade_no,
            &[ApiWithdrawStatus::Failure, ApiWithdrawStatus::Success],
        )
        .await;
        match res {
            Ok(res) => self.process_withdraw_single_tx_confirm_report(res).await,
            Err(err) => {
                tracing::warn!(trade_no=%trade_no, "process withdraw single tx report by id: {:?}", err);
            }
        }
    }

    async fn process_withdraw_tx_confirm_report(&mut self) {
        let res = ApiWithdrawRepo::list_api_withdraw_with_status(
            &self.pool,
            vec![ApiWithdrawStatus::Failure, ApiWithdrawStatus::Success],
            0,
            1000,
        )
        .await;
        match res {
            Ok(res) => {
                for req in res {
                    self.process_withdraw_single_tx_confirm_report(req).await
                }
            }
            Err(err) => {
                tracing::warn!("process withdraw single tx report by id: {:?}", err);
            }
        }
    }

    async fn process_withdraw_single_tx_confirm_report(&self, req: ApiWithdrawEntity) {
        tracing::info!(trade_no=%req.trade_no,hash=%req.tx_hash,status=%req.status, "process_withdraw_single_tx_confirm_report ---------------------------------4");
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(req.post_confirm_tx_count as i64) {
            tracing::warn!(trade_no=%req.trade_no,
                "process_withdraw_single_tx_confirm_report timeout post confirm_tx_count is too long"
            );
            return;
        }
        if req.status == ApiWithdrawStatus::SendingTxFailed {
            tracing::warn!(trade_no=%req.trade_no, "process_withdraw_single_tx_confirm_report status is wrong");
            return;
        };
        if !(req.status == ApiWithdrawStatus::Success || req.status == ApiWithdrawStatus::Failure) {
            tracing::warn!(trade_no=%req.trade_no,
                "process_withdraw_single_tx_confirm_report status is wrong {}",
                req.status
            );
            return;
        }
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend_api
            .trans_event_ack(&TransEventAckReq::new(
                &req.trade_no,
                TransType::Wd,
                TransAckType::TxRes,
            ))
            .await
        {
            Ok(_) => self.handle_confirm_report_success(req).await,
            Err(err) => self.handle_confirm_report_failed(req, err).await,
        }
    }

    async fn handle_confirm_report_success(&self, req: ApiWithdrawEntity) {
        let (next_status, notes) = if req.status == ApiWithdrawStatus::Success {
            (ApiWithdrawStatus::ConfirmSuccessReport, "withdraw trans event ack success")
        } else {
            (ApiWithdrawStatus::ConfirmFailureReport, "withdraw trans event ack failure")
        };
        tracing::info!(trade_no=%req.trade_no, "process_withdraw_single_tx_confirm_report success");
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
                }
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "process withdraw single tx report by id: {:?}", err);
            }
        }
    }

    async fn handle_confirm_report_failed(
        &self,
        req: ApiWithdrawEntity,
        err: wallet_transport_backend::Error,
    ) {
        tracing::error!(trade_no=%req.trade_no, "failed to process withdraw tx confirm report: {}", err);
        let res = ApiWithdrawRepo::update_api_withdraw_post_confirm_tx_count(
            &self.pool,
            &req.trade_no,
            req.status,
        )
        .await;
        match res {
            Ok(res) => {}
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "process withdraw tx report by id: {:?}", err);
            }
        }
    }
}
