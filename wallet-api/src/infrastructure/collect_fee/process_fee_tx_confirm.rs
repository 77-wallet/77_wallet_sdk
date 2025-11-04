use crate::{
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    infrastructure::collect_fee::command::ProcessFeeTxConfirmReportCommand,
};
use chrono::TimeDelta;
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
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessFeeTxConfirmReportCommand>,
    failed_count: i64,
}

impl ProcessFeeTxConfirmReport {
    pub(super) fn new(
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessFeeTxConfirmReportCommand>,
    ) -> Self {
        Self { shutdown_rx, report_rx, failed_count: 0 }
    }

    pub(super) async fn run(&mut self) -> Result<(), ServiceError> {
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
                                    match self.process_fee_single_tx_confirm_report_by_trade_no(&trade_no).await {
                                        Ok(_) => {}
                                        Err(err) => {
                                            tracing::error!("failed to process fee tx confirm report: {}", err);
                                        }
                                    }
                                    iv.reset();
                                }
                            }
                        }
                        None => {}
                    }
                }
                _ = iv.tick() => {
                    match self.process_fee_tx_confirm_report().await {
                        Ok(_) => {}
                        Err(err) => {
                            tracing::error!("failed to process fee tx confirm report: {}", err);
                        }
                    }
                }
            }
        }
        tracing::info!("closing process fee tx confirm report ------------------------------- end");
        Ok(())
    }

    async fn process_fee_single_tx_confirm_report_by_trade_no(
        &self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiFeeRepo::get_api_fee_by_trade_no(&pool, &trade_no).await;
        if res.is_ok() {
            self.process_fee_single_tx_confirm_report(res.unwrap()).await?;
            Ok(())
        } else {
            Err(ServiceError::Business(ApiWalletError::OrderNotFound(trade_no.to_string()).into()))
        }
    }

    async fn process_fee_tx_confirm_report(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let (_, transfer_fees) = ApiFeeRepo::page_api_fee_with_status(
            &pool,
            0,
            1000 + self.failed_count,
            &[ApiFeeStatus::Failure, ApiFeeStatus::Success],
        )
        .await?;
        let transfer_fees_len = transfer_fees.len();
        let mut failed_count = 0;
        for req in transfer_fees {
            if let Err(_) = self.process_fee_single_tx_confirm_report(req).await {
                failed_count += 1;
            }
        }
        if failed_count == transfer_fees_len as i32 {
            self.failed_count += 1;
        }
        Ok(())
    }

    async fn process_fee_single_tx_confirm_report(
        &self,
        req: ApiFeeEntity,
    ) -> Result<(), ServiceError> {
        tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_fee_single_tx_confirm_report ---------------------------------4");
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(req.post_confirm_tx_count as i64) {
            tracing::warn!(
                "process_fee_single_tx_confirm_report timeout post confirm_tx_count is too long"
            );
            return Ok(());
        }
        if req.status == ApiFeeStatus::SendingTxFailed {
            tracing::warn!("process_fee_single_tx_confirm_report status is wrong");
            return Ok(());
        };
        if !(req.status == ApiFeeStatus::Success || req.status == ApiFeeStatus::Failure) {
            tracing::warn!("process_fee_single_tx_confirm_report status is wrong {}", req.status);
            return Ok(());
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
            Ok(_) => {
                let next_status = if req.status == ApiFeeStatus::Success {
                    ApiFeeStatus::ConfirmSuccessReport
                } else {
                    ApiFeeStatus::ConfirmFailureReport
                };
                tracing::info!("process_fee_single_tx_confirm_report success");
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                ApiFeeRepo::update_api_fee_next_status(
                    &pool,
                    &req.trade_no,
                    req.status,
                    next_status,
                    "fee trans event ack",
                )
                .await?;
                return Ok(());
            }
            Err(err) => {
                tracing::error!("failed to process fee tx confirm report: {}", err);
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                ApiFeeRepo::update_api_fee_post_confirm_tx_count(&pool, &req.trade_no, req.status)
                    .await?;
                return Err(ServiceError::TransportBackend(err.into()));
            }
        }
        Ok(())
    }
}
