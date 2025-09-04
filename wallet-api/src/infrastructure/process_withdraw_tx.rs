use crate::{
    Context,
    domain::{api_wallet::trans::ApiTransDomain, chain::TransferResp, coin::CoinDomain},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use chrono::TimeDelta;
use tokio::{
    sync::{Mutex, broadcast, mpsc},
    task::JoinHandle,
};
use wallet_database::{
    entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
    repositories::api_withdraw::ApiWithdrawRepo,
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransStatus, TransType, TxExecReceiptUploadReq,
};

#[derive(Clone)]
pub(crate) enum ProcessWithdrawTxCommand {
    Tx(String),
}

#[derive(Clone)]
pub(crate) enum ProcessWithdrawTxReportCommand {
    Tx,
}

#[derive(Clone)]
pub(crate) enum ProcessWithdrawTxConfirmReportCommand {
    Tx,
}

#[derive(Debug)]
pub(crate) struct ProcessWithdrawTxHandle {
    shutdown_tx: broadcast::Sender<()>,
    tx_tx: mpsc::Sender<ProcessWithdrawTxCommand>,
    confirm_report_tx: mpsc::Sender<ProcessWithdrawTxConfirmReportCommand>,
    tx_handle: Mutex<Option<JoinHandle<Result<(), crate::ServiceError>>>>,
    tx_report_handle: Mutex<Option<JoinHandle<Result<(), crate::ServiceError>>>>,
    tx_confirm_report_handle: Mutex<Option<JoinHandle<Result<(), crate::ServiceError>>>>,
}

impl ProcessWithdrawTxHandle {
    pub(crate) async fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let shutdown_rx1 = shutdown_tx.subscribe();
        let shutdown_rx2 = shutdown_tx.subscribe();
        let shutdown_rx3 = shutdown_tx.subscribe();
        let (report_tx, report_rx) = mpsc::channel(1);
        // 发交易
        let (tx_tx, tx_rx) = mpsc::channel(1);
        let mut tx = ProcessWithdrawTx::new(shutdown_rx1, tx_rx, report_tx);
        let handle = tokio::spawn(async move { tx.run().await });
        // 上报交易
        let mut tx_report = ProcessWithdrawTxReport::new(shutdown_rx2, report_rx);
        let tx_report_handle = tokio::spawn(async move { tx_report.run().await });
        // 上报已经确认交易
        let (confirm_report_tx, confirm_report_rx) = mpsc::channel(1);
        let mut tx_confirm_report =
            ProcessWithdrawTxConfirmReport::new(shutdown_rx3, confirm_report_rx);
        let tx_confirm_report_handle = tokio::spawn(async move { tx_confirm_report.run().await });
        Self {
            shutdown_tx,
            tx_tx: tx_tx,
            confirm_report_tx,
            tx_handle: Mutex::new(Some(handle)),
            tx_report_handle: Mutex::new(Some(tx_report_handle)),
            tx_confirm_report_handle: Mutex::new(Some(tx_confirm_report_handle)),
        }
    }

    pub(crate) async fn submit_tx(&self, trade_no: &str) -> Result<(), crate::ServiceError> {
        let _ = self.tx_tx.send(ProcessWithdrawTxCommand::Tx(trade_no.to_string()));
        Ok(())
    }

    pub(crate) async fn submit_confirm_report_tx(&self) -> Result<(), crate::ServiceError> {
        let _ = self.confirm_report_tx.send(ProcessWithdrawTxConfirmReportCommand::Tx);
        Ok(())
    }

    pub(crate) async fn close(&self) -> Result<(), crate::ServiceError> {
        let _ = self.shutdown_tx.send(());
        if let Some(handle) = self.tx_handle.lock().await.take() {
            handle.await.map_err(|_| {
                crate::ServiceError::System(crate::SystemError::BackendEndpointNotFound)
            })??;
        }
        if let Some(handle) = self.tx_report_handle.lock().await.take() {
            handle.await.map_err(|_| {
                crate::ServiceError::System(crate::SystemError::BackendEndpointNotFound)
            })??;
        }
        if let Some(handle) = self.tx_confirm_report_handle.lock().await.take() {
            handle.await.map_err(|_| {
                crate::ServiceError::System(crate::SystemError::BackendEndpointNotFound)
            })??;
        }
        Ok(())
    }
}

struct ProcessWithdrawTx {
    shutdown_rx: broadcast::Receiver<()>,
    tx_rx: mpsc::Receiver<ProcessWithdrawTxCommand>,
    report_tx: mpsc::Sender<ProcessWithdrawTxReportCommand>,
}

impl ProcessWithdrawTx {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        tx_rx: mpsc::Receiver<ProcessWithdrawTxCommand>,
        report_tx: mpsc::Sender<ProcessWithdrawTxReportCommand>,
    ) -> Self {
        Self { shutdown_rx, tx_rx, report_tx }
    }

    async fn run(&mut self) -> Result<(), crate::ServiceError> {
        tracing::info!("starting process withdraw -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process withdraw tx -------------------------------");
                    break;
                }
                msg = self.tx_rx.recv() => {
                    if let Some(cmd) = msg {
                        match cmd {
                            ProcessWithdrawTxCommand::Tx(trade_no) => {
                                let pool = crate::manager::Context::get_global_sqlite_pool()?;
                                let res = ApiWithdrawRepo::get_api_withdraw_by_trade_no(&pool, &trade_no).await;
                                if res.is_ok() {
                                    match self.process_withdraw_single_tx(res.unwrap()).await {
                                        Ok(_) => {}
                                        Err(_) => {
                                            tracing::error!("failed to process withdraw tx");
                                        }
                                    }
                                }
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    match self.process_withdraw_tx().await {
                        Ok(_) => {}
                        Err(_) => {
                            tracing::error!("failed to process withdraw tx");
                        }
                    }
                }
            }
        }
        tracing::info!("closing process withdraw tx ------------------------------- end");
        Ok(())
    }

    async fn process_withdraw_tx(&self) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let (_, withdraws) = ApiWithdrawRepo::page_api_withdraw_with_status(
            &pool.clone(),
            0,
            1000,
            &[ApiWithdrawStatus::AuditPass],
        )
        .await?;
        for req in withdraws {
            self.process_withdraw_single_tx(req).await?;
        }
        Ok(())
    }

    async fn process_withdraw_single_tx(
        &self,
        req: ApiWithdrawEntity,
    ) -> Result<i32, crate::ServiceError> {
        tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_withdraw_single_tx ---------------------------------4");

        let coin =
            CoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_addr.clone()).await?;

        let mut params = ApiBaseTransferReq::new(
            &req.from_addr,
            &req.to_addr.to_string(),
            &req.value.to_string(),
            &req.chain_code.to_string(),
        );
        let token_address = if coin.token_address.is_none() {
            None
        } else {
            let s = coin.token_address.unwrap();
            if s.is_empty() { None } else { Some(s) }
        };
        params.with_token(token_address, coin.decimals, &coin.symbol);

        let transfer_req = ApiTransferReq { base: params, password: "q1111111".to_string() };

        // 发交易
        let tx_resp = ApiTransDomain::transfer(transfer_req).await;
        match tx_resp {
            Ok(tx) => self.handle_withdraw_tx_success(&req.trade_no, tx).await,
            Err(_) => self.handle_withdraw_tx_failed(&req.trade_no).await,
        }
    }

    async fn handle_withdraw_tx_success(
        &self,
        trade_no: &str,
        tx: TransferResp,
    ) -> Result<i32, crate::ServiceError> {
        let resource_consume = if tx.consumer.is_none() {
            "0".to_string()
        } else {
            tx.consumer.unwrap().energy_used.to_string()
        };
        // 更新交易状态
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_tx_status(
            &pool,
            trade_no,
            &tx.tx_hash,
            &resource_consume,
            &tx.fee,
            ApiWithdrawStatus::SendingTx,
        )
        .await?;

        // 上报交易
        let _ = self.report_tx.send(ProcessWithdrawTxReportCommand::Tx);
        Ok(1)
    }

    async fn handle_withdraw_tx_failed(&self, trade_no: &str) -> Result<i32, crate::ServiceError> {
        // 更新交易状态,发送失败
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status(
            &pool,
            trade_no,
            ApiWithdrawStatus::SendingTxFailed,
        )
        .await?;
        // 上报交易
        let _ = self.report_tx.send(ProcessWithdrawTxReportCommand::Tx);
        Ok(1)
    }
}

struct ProcessWithdrawTxReport {
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessWithdrawTxReportCommand>,
    failed_count: i64,
}

impl ProcessWithdrawTxReport {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessWithdrawTxReportCommand>,
    ) -> Self {
        Self { shutdown_rx, report_rx, failed_count: 0 }
    }

    async fn run(&mut self) -> Result<(), crate::ServiceError> {
        tracing::info!("starting process withdraw tx report -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process withdraw tx report -------------------------------");
                    break;
                }
                msg = self.report_rx.recv() => {
                    if let Some(cmd) = msg {
                        match cmd {
                            ProcessWithdrawTxReportCommand::Tx => {
                                match self.process_withdraw_tx_report().await {
                                    Ok(_) => {},
                                    Err(_) => {
                                        tracing::error!("failed to process withdraw tx report");
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

    async fn process_withdraw_tx_report(&mut self) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let (_, transfer_fees) = ApiWithdrawRepo::page_api_withdraw_with_status(
            &pool,
            0,
            1000 + self.failed_count,
            &[ApiWithdrawStatus::SendingTx, ApiWithdrawStatus::SendingTxFailed],
        )
        .await?;
        let transfer_fees_len = transfer_fees.len();
        let mut failed_count = 0;
        for req in transfer_fees {
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
    ) -> Result<(), crate::ServiceError> {
        tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_withdraw_single_tx_report ---------------------------------4");
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(req.post_tx_count as i64) {
            return Ok(());
        }
        let status = if req.status == ApiWithdrawStatus::SendingTxFailed {
            TransStatus::Fail
        } else {
            TransStatus::Success
        };
        let backend_api = Context::get_global_backend_api()?;
        match backend_api
            .upload_tx_exec_receipt(&TxExecReceiptUploadReq::new(
                &req.trade_no,
                TransType::Fee,
                &req.tx_hash,
                status,
                &req.notes,
            ))
            .await
        {
            Ok(_) => {
                let pool = crate::manager::Context::get_global_sqlite_pool()?;
                ApiWithdrawRepo::update_api_withdraw_next_status(
                    &pool,
                    &req.trade_no,
                    ApiWithdrawStatus::SendingTx,
                    ApiWithdrawStatus::ReceivedTxReport,
                )
                .await?;
                tracing::info!("upload tx exec receipt success ---");
                return Ok(());
            }
            Err(err) => {
                let pool = crate::manager::Context::get_global_sqlite_pool()?;
                ApiWithdrawRepo::update_api_fee_post_tx_count(
                    &pool,
                    &req.trade_no,
                    ApiWithdrawStatus::SendingTx,
                )
                .await?;
                return Err(crate::ServiceError::TransportBackend(err.into()));
            }
        }
    }
}

struct ProcessWithdrawTxConfirmReport {
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessWithdrawTxConfirmReportCommand>,
    failed_count: i64,
}

impl ProcessWithdrawTxConfirmReport {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessWithdrawTxConfirmReportCommand>,
    ) -> Self {
        Self { shutdown_rx, report_rx, failed_count: 0 }
    }

    async fn run(&mut self) -> Result<(), crate::ServiceError> {
        tracing::info!(
            "starting process withdraw tx confirm report -------------------------------"
        );
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process withdraw tx confirm report -------------------------------");
                    break;
                }
                msg = self.report_rx.recv() => {
                    if let Some(cmd) = msg {
                        match cmd {
                            ProcessWithdrawTxConfirmReportCommand::Tx => {}
                        }
                        iv.reset();
                    }
                }
                _ = iv.tick() => {
                    match self.process_withdraw_tx_confirm_report().await {
                        Ok(_) => {}
                        Err(_) => {
                            tracing::error!("failed to process withdraw tx confirm report");
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

    async fn process_withdraw_tx_confirm_report(&mut self) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let (_, withdraws) = ApiWithdrawRepo::page_api_withdraw_with_status(
            &pool,
            0,
            1000 + self.failed_count,
            &[ApiWithdrawStatus::Failure, ApiWithdrawStatus::Success],
        )
        .await?;
        let withdraws_len = withdraws.len();
        let mut failed_count = 0;
        for req in withdraws {
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
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status(
            &pool,
            &req.trade_no,
            ApiWithdrawStatus::Success,
        )
        .await?;
        Ok(())
    }
}
