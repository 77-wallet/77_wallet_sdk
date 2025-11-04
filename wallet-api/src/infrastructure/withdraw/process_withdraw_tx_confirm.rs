use crate::{
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    infrastructure::withdraw::command::ProcessWithdrawTxConfirmReportCommand,
};
use chrono::TimeDelta;
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
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessWithdrawTxConfirmReportCommand>,
    failed_count: i64,
}

impl ProcessWithdrawTxConfirmReport {
    pub(super) fn new(
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessWithdrawTxConfirmReportCommand>,
    ) -> Self {
        Self { shutdown_rx, report_rx, failed_count: 0 }
    }

    pub(super) async fn run(&mut self) -> Result<(), ServiceError> {
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
                                match self.process_withdraw_single_tx_confirm_report_by_trade_no(&trade_no).await {
                                    Ok(_) => {}
                                    Err(err) => {
                                        tracing::error!("failed to process withdraw tx confirm report: {:?}", err);
                                    }
                                }
                            }
                        }
                        iv.reset();
                    }
                }
                _ = iv.tick() => {
                    match self.process_withdraw_tx_confirm_report().await {
                        Ok(_) => {}
                        Err(err) => {
                            tracing::error!("failed to process withdraw tx confirm report: {:?}", err);
                        }
                    }
                }
            }
        }
        tracing::info!(
            "closing process withdraw tx confirm report ------------------------------- end"
        );
        Ok(())
    }

    async fn process_withdraw_single_tx_confirm_report_by_trade_no(
        &self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiWithdrawRepo::get_api_withdraw_by_trade_no_status(
            &pool,
            trade_no,
            &[ApiWithdrawStatus::Failure, ApiWithdrawStatus::Success],
        )
        .await;
        if res.is_ok() {
            self.process_withdraw_single_tx_confirm_report(res.unwrap()).await?;
            Ok(())
        } else {
            Err(ServiceError::Business(ApiWalletError::OrderNotFound(trade_no.to_string()).into()))
        }
    }

    async fn process_withdraw_tx_confirm_report(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiWithdrawRepo::list_api_withdraw_with_status(
            &pool,
            vec![ApiWithdrawStatus::Failure, ApiWithdrawStatus::Success],
            0,
            1000 + self.failed_count,
        )
        .await?;
        let withdraws_len = res.len();
        let mut failed_count = 0;
        for req in res {
            if let Err(_) = self.process_withdraw_single_tx_confirm_report(req).await {
                failed_count += 1;
            }
        }
        if failed_count == withdraws_len as i32 {
            self.failed_count += 1;
        }
        Ok(())
    }

    async fn process_withdraw_single_tx_confirm_report(
        &self,
        req: ApiWithdrawEntity,
    ) -> Result<(), ServiceError> {
        tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_withdraw_single_tx_confirm_report ---------------------------------4");
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(req.post_confirm_tx_count as i64) {
            tracing::warn!(
                "process_withdraw_single_tx_confirm_report timeout post confirm_tx_count is too long"
            );
            return Ok(());
        }
        if req.status == ApiWithdrawStatus::SendingTxFailed {
            tracing::warn!("process_withdraw_single_tx_confirm_report status is wrong");
            return Ok(());
        };
        if !(req.status == ApiWithdrawStatus::Success || req.status == ApiWithdrawStatus::Failure) {
            tracing::warn!(
                "process_withdraw_single_tx_confirm_report status is wrong {}",
                req.status
            );
            return Ok(());
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
            Ok(_) => {
                let next_status = if req.status == ApiWithdrawStatus::Success {
                    ApiWithdrawStatus::ConfirmSuccessReport
                } else {
                    ApiWithdrawStatus::ConfirmFailureReport
                };
                tracing::info!("process_withdraw_single_tx_confirm_report success");
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                ApiWithdrawRepo::update_api_withdraw_next_status(
                    &pool,
                    &req.trade_no,
                    req.status,
                    next_status,
                    "withdraw trans event ack",
                )
                .await?;
                return Ok(());
            }
            Err(err) => {
                tracing::error!("failed to process withdraw tx confirm report: {}", err);
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                ApiWithdrawRepo::update_api_withdraw_post_confirm_tx_count(
                    &pool,
                    &req.trade_no,
                    req.status,
                )
                .await?;
                return Err(ServiceError::TransportBackend(err.into()));
            }
        }
        Ok(())
    }
}
