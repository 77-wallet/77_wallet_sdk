use crate::{
    error::service::ServiceError,
    infrastructure::collect::command::ProcessCollectTxConfirmReportCommand,
};
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
    TransAckType, TransEventAckReq, TransType,
};

pub(super) struct ProcessCollectTxConfirmReport {
    pool: Arc<sqlx::SqlitePool>,
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessCollectTxConfirmReportCommand>,
}

impl ProcessCollectTxConfirmReport {
    pub(super) fn new(
        pool: Arc<sqlx::SqlitePool>,
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessCollectTxConfirmReportCommand>,
    ) -> Self {
        Self { pool, shutdown_rx, report_rx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!(
            "starting process collect tx confirm report -------------------------------"
        );
        let res = self.run_with_err().await;
        match res {
            Ok(_) => {
                tracing::info!("closing process collect tx confirm report ------------- end");
            }
            Err(err) => {
                tracing::error!("failed to process collect tx confirm report failed: {}", err);
            }
        }
    }

    async fn run_with_err(&mut self) -> Result<(), ServiceError> {
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process collect tx confirm report -------------------------------");
                    break;
                }
                report_msg = self.report_rx.recv() => {
                    match report_msg {
                        Some(cmd) => {
                            match cmd {
                                ProcessCollectTxConfirmReportCommand::Tx(trade_no) => {
                                    self.process_fee_single_tx_confirm_report_by_trade_no(&trade_no).await;
                                    iv.reset();
                                }
                            }
                        }
                        None => {}
                    }
                }
                _ = iv.tick() => {
                    self.process_collect_tx_confirm_report().await
                }
            }
        }
        Ok(())
    }

    async fn process_fee_single_tx_confirm_report_by_trade_no(&self, trade_no: &str) {
        let res = ApiCollectRepo::get_api_collect_by_trade_no_status(
            &self.pool,
            &trade_no,
            &[ApiCollectStatus::Failure, ApiCollectStatus::Success],
        )
        .await;
        match res {
            Ok(res) => self.process_collect_single_tx_confirm_report(res).await,
            Err(err) => {
                tracing::warn!(trade_no=%trade_no, "failed to process collect tx confirm report failed: {}", err);
            }
        }
    }

    async fn process_collect_tx_confirm_report(&mut self) {
        let res = ApiCollectRepo::page_api_collect_with_status(
            &self.pool,
            0,
            1000,
            &[ApiCollectStatus::Failure, ApiCollectStatus::Success],
        )
        .await;
        match res {
            Ok((_, transfer_fees)) => {
                for req in transfer_fees {
                    self.process_collect_single_tx_confirm_report(req).await
                }
            }
            Err(err) => {
                tracing::warn!("failed to process collect tx confirm report failed: {}", err);
            }
        }
    }

    async fn process_collect_single_tx_confirm_report(&self, req: ApiCollectEntity) {
        tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_collect_single_tx_confirm_report ---------------------------------4");
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(req.post_confirm_tx_count as i64) {
            tracing::warn!(
                "process_withdraw_single_tx_confirm_report timeout post confirm_tx_count is too long"
            );
            return;
        }
        if req.status == ApiCollectStatus::SendingTxFailed {
            tracing::warn!("process_withdraw_single_tx_confirm_report status is wrong");
            return;
        };
        if !(req.status == ApiCollectStatus::Success || req.status == ApiCollectStatus::Failure) {
            tracing::warn!(
                "process_collect_single_tx_confirm_report status is wrong {}",
                req.status
            );
            return;
        }
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend_api
            .trans_event_ack(&TransEventAckReq::new(
                &req.trade_no,
                TransType::Col,
                TransAckType::TxRes,
            ))
            .await
        {
            Ok(_) => self.handle_confirm_report_success(req).await,
            Err(err) => self.handle_confirm_report_failed(req, err).await,
        }
    }

    async fn handle_confirm_report_success(&self, req: ApiCollectEntity) {
        let next_status = if req.status == ApiCollectStatus::Success {
            ApiCollectStatus::ConfirmSuccessReport
        } else {
            ApiCollectStatus::ConfirmFailureReport
        };
        tracing::info!("process_collect_single_tx_confirm_report success");
        let res = ApiCollectRepo::update_api_collect_next_status(
            &self.pool,
            &req.trade_no,
            req.status,
            next_status,
            "trans event ack",
        )
        .await;
        match res {
            Ok(_) => {}
            Err(err) => {
                tracing::error!("failed to process collect tx confirm report failed: {}", err);
            }
        }
    }

    async fn handle_confirm_report_failed(
        &self,
        req: ApiCollectEntity,
        err: wallet_transport_backend::Error,
    ) {
        tracing::error!("failed to process withdraw tx confirm report: {}", err);
        let res = ApiCollectRepo::update_api_collect_post_confirm_tx_count(
            &self.pool,
            &req.trade_no,
            req.status,
        )
        .await;
        match res {
            Ok(_) => {}
            Err(err) => {
                tracing::error!("failed to process withdraw tx confirm report failed: {}", err);
            }
        }
    }
}
