use crate::{
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    infrastructure::withdraw::command::ProcessWithdrawTxReportCommand,
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
    TransStatus, TransType, TxExecReceiptUploadReq,
};

pub(super) struct ProcessWithdrawTxReport {
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessWithdrawTxReportCommand>,
    failed_count: i64,
}

impl ProcessWithdrawTxReport {
    pub(super) fn new(
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessWithdrawTxReportCommand>,
    ) -> Self {
        Self { shutdown_rx, report_rx, failed_count: 0 }
    }

    pub(super) async fn run(&mut self) -> Result<(), ServiceError> {
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
                                match self.process_withdraw_single_tx_report_by_trade_no(&trade_no).await {
                                    Ok(_) => {},
                                    Err(_) => {
                                        tracing::error!("failed to process single withdraw tx report");
                                    }
                                }
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    match self.process_withdraw_tx_report().await {
                        Ok(_) => {}
                        Err(_) => {
                            tracing::error!("failed to process withdraw tx report");
                        }
                    }
                }
            }
        }
        tracing::info!("closing process withdraw tx report ------------------------------- end");
        Ok(())
    }

    async fn process_withdraw_single_tx_report_by_trade_no(
        &mut self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiWithdrawRepo::get_api_withdraw_by_trade_no_status(
            &pool,
            &trade_no,
            &[ApiWithdrawStatus::SendingTx, ApiWithdrawStatus::SendingTxFailed],
        )
        .await;
        if res.is_ok() {
            self.process_withdraw_single_tx_report(res.unwrap()).await?;
            Ok(())
        } else {
            Err(ServiceError::Business(ApiWalletError::OrderNotFound(trade_no.to_string()).into()))
        }
    }

    async fn process_withdraw_tx_report(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiWithdrawRepo::list_api_withdraw_with_status(
            &pool,
            vec![ApiWithdrawStatus::SendingTx, ApiWithdrawStatus::SendingTxFailed],
            0,
            1000 + self.failed_count,
        )
        .await?;
        let transfer_fees_len = res.len();
        let mut failed_count = 0;
        for req in res {
            if let Err(_) = self.process_withdraw_single_tx_report(req).await {
                failed_count += 1;
            }
        }
        if failed_count == transfer_fees_len as i32 {
            self.failed_count += 1;
        }
        Ok(())
    }

    async fn process_withdraw_single_tx_report(
        &self,
        req: ApiWithdrawEntity,
    ) -> Result<(), ServiceError> {
        tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_withdraw_single_tx_report ---------------------------------4");
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(req.post_tx_count as i64) {
            tracing::warn!(
                "process_withdraw_single_tx_report timed out, post_tx_count: {}, timeout: {}",
                req.post_tx_count,
                timeout
            );
            return Ok(());
        }
        // 转成服务需要的状态
        let status = if req.status == ApiWithdrawStatus::SendingTxFailed {
            TransStatus::Fail
        } else {
            TransStatus::Success
        };
        let tx_exec_receipt_upload_req = TxExecReceiptUploadReq::new(
            &req.trade_no,
            TransType::Wd,
            &req.tx_hash,
            status,
            &req.notes,
        );
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend_api.upload_tx_exec_receipt(&tx_exec_receipt_upload_req).await {
            Ok(_) => {
                tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_withdraw_single_tx_report ok");
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                if req.status == ApiWithdrawStatus::SendingTxFailed {
                    ApiWithdrawRepo::update_api_withdraw_next_status(
                        &pool,
                        &req.trade_no,
                        ApiWithdrawStatus::SendingTxFailed,
                        ApiWithdrawStatus::SendingTxFailedReport,
                        "upload server ok for withdraw send tx failed",
                    )
                    .await?;
                } else {
                    // 发送交易结果确认
                    ApiWithdrawRepo::update_api_withdraw_next_status(
                        &pool,
                        &req.trade_no,
                        ApiWithdrawStatus::SendingTx,
                        ApiWithdrawStatus::SendingTxReport,
                        "upload server ok for withdraw success",
                    )
                    .await?;
                }
                tracing::info!("upload tx exec receipt success ---");
                Ok(())
            }
            Err(err) => {
                tracing::error!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_withdraw_single_tx_report -----------{}", err);
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                ApiWithdrawRepo::update_api_fee_post_tx_count(&pool, &req.trade_no, req.status)
                    .await?;
                Err(ServiceError::TransportBackend(err.into()))
            }
        }
    }
}
