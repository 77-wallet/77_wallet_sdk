use crate::{
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    infrastructure::collect_fee::command::ProcessFeeTxReportCommand,
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
    TransStatus, TransType, TxExecReceiptUploadReq,
};

pub(super) struct ProcessFeeTxReport {
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessFeeTxReportCommand>,
    failed_count: i64,
}

impl ProcessFeeTxReport {
    pub(super) fn new(
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessFeeTxReportCommand>,
    ) -> Self {
        Self { shutdown_rx, report_rx, failed_count: 0 }
    }

    pub(super) async fn run(&mut self) -> Result<(), ServiceError> {
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
                                match self.process_fee_single_tx_report_by_trade_no(&trade_no).await {
                                    Ok(_) => {}
                                    Err(err) => {
                                        tracing::error!("failed to process fee tx report: {}", err);
                                    }
                                }
                            }
                        }
                        iv.reset();
                    }
                }
                _ = iv.tick() => {
                    match self.process_fee_tx_report().await {
                        Ok(_) => {}
                        Err(err) => {
                            tracing::error!("failed to process fee tx report: {}", err);
                        }
                    }
                }
            }
        }
        tracing::info!("closing process fee tx report ------------------------------- end");
        Ok(())
    }

    async fn process_fee_single_tx_report_by_trade_no(
        &self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiFeeRepo::get_api_fee_by_trade_no_status(
            &pool,
            &trade_no,
            &[ApiFeeStatus::SendingTx, ApiFeeStatus::SendingTxFailed],
        )
        .await;
        if res.is_ok() {
            self.process_fee_single_tx_report(res.unwrap()).await?;
            Ok(())
        } else {
            Err(ServiceError::Business(ApiWalletError::OrderNotFound(trade_no.to_string()).into()))
        }
    }

    async fn process_fee_tx_report(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let (_, transfer_fees) = ApiFeeRepo::page_api_fee_with_status(
            &pool,
            0,
            1000 + self.failed_count,
            &[ApiFeeStatus::SendingTx, ApiFeeStatus::SendingTxFailed],
        )
        .await?;
        let transfer_fees_len = transfer_fees.len();
        let mut failed_count = 0;
        for req in transfer_fees {
            if let Err(_) = self.process_fee_single_tx_report(req).await {
                failed_count += 1;
            }
        }
        if failed_count == transfer_fees_len as i32 {
            self.failed_count += 1;
        }
        Ok(())
    }

    async fn process_fee_single_tx_report(&self, req: ApiFeeEntity) -> Result<i32, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "process fee tx report -------------------------------");
        // 判断超时时间
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(1 << req.post_tx_count as i64) {
            tracing::warn!(trade_no=%req.trade_no, "process fee tx report timeout ---");
            return Ok(0);
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
                &req.notes,
            ))
            .await
        {
            Ok(_) => {
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                if req.status == ApiFeeStatus::SendingTxFailed {
                    ApiFeeRepo::update_api_fee_next_status(
                        &pool,
                        &req.trade_no,
                        ApiFeeStatus::SendingTxFailed,
                        ApiFeeStatus::SendingTxFailedReport,
                        "upload server ok for transfer fee send tx failed",
                    )
                    .await?;
                } else {
                    ApiFeeRepo::update_api_fee_next_status(
                        &pool,
                        &req.trade_no,
                        ApiFeeStatus::SendingTx,
                        ApiFeeStatus::SendingTxReport,
                        "upload server ok for transfer fee success",
                    )
                    .await?;
                }
                tracing::info!("upload tx exec receipt success ---");
                Ok(1)
            }
            Err(err) => {
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                if req.status == ApiFeeStatus::SendingTx {
                    ApiFeeRepo::update_api_fee_post_tx_count(
                        &pool,
                        &req.trade_no,
                        ApiFeeStatus::SendingTx,
                    )
                    .await?;
                } else {
                    ApiFeeRepo::update_api_fee_post_tx_count(
                        &pool,
                        &req.trade_no,
                        ApiFeeStatus::SendingTxFailed,
                    )
                    .await?;
                }
                Err(ServiceError::TransportBackend(err))
            }
        }
    }
}
