use crate::{
    domain::{
        api_wallet::{trans::ApiTransDomain, wallet::ApiWalletDomain},
        chain::TransferResp,
        coin::CoinDomain,
    },
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use chrono::TimeDelta;
use tokio::{
    sync::{Mutex, broadcast, mpsc},
    task::JoinHandle,
};
use wallet_database::{
    entities::api_fee::{ApiFeeEntity, ApiFeeStatus},
    repositories::api_wallet::fee::ApiFeeRepo,
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransStatus, TransType, TxExecReceiptUploadReq,
};

#[derive(Clone)]
pub(crate) enum ProcessFeeTxCommand {
    Tx(String),
}

#[derive(Clone)]
pub(crate) enum ProcessFeeTxReportCommand {
    Tx(String),
}

#[derive(Clone)]
pub(crate) enum ProcessFeeTxConfirmReportCommand {
    Tx(String),
}

#[derive(Debug)]
pub(crate) struct ProcessFeeTxHandle {
    shutdown_tx: broadcast::Sender<()>,
    tx_tx: mpsc::Sender<ProcessFeeTxCommand>,
    confirm_report_tx: mpsc::Sender<ProcessFeeTxConfirmReportCommand>,
    tx_handle: Mutex<Option<JoinHandle<Result<(), crate::error::service::ServiceError>>>>,
    tx_report_handle: Mutex<Option<JoinHandle<Result<(), crate::error::service::ServiceError>>>>,
    tx_confirm_report_handle:
        Mutex<Option<JoinHandle<Result<(), crate::error::service::ServiceError>>>>,
}

impl ProcessFeeTxHandle {
    pub(crate) async fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let shutdown_rx1 = shutdown_tx.subscribe();
        let shutdown_rx2 = shutdown_tx.subscribe();
        let shutdown_rx3 = shutdown_tx.subscribe();
        let (tx_tx, tx_rx) = mpsc::channel(1);
        let (report_tx, report_rx) = mpsc::channel(1);
        // 发交易
        let mut tx = ProcessFeeTx::new(shutdown_rx1, tx_rx, report_tx);
        let tx_handle = tokio::spawn(async move { tx.run().await });
        // 上报交易
        let mut tx_report = ProcessFeeTxReport::new(shutdown_rx2, report_rx);
        let tx_report_handle = tokio::spawn(async move { tx_report.run().await });
        // 上报已经确认交易
        let (confirm_report_tx, confirm_report_rx) = mpsc::channel(1);
        let mut tx_confirm_report = ProcessFeeTxConfirmReport::new(shutdown_rx3, confirm_report_rx);
        let tx_confirm_report_handle = tokio::spawn(async move { tx_confirm_report.run().await });
        Self {
            shutdown_tx: shutdown_tx,
            tx_tx: tx_tx,
            confirm_report_tx,
            tx_handle: Mutex::new(Some(tx_handle)),
            tx_report_handle: Mutex::new(Some(tx_report_handle)),
            tx_confirm_report_handle: Mutex::new(Some(tx_confirm_report_handle)),
        }
    }

    pub(crate) async fn submit_tx(&self, trade_no: &str) -> Result<(), ServiceError> {
        let _ = self.tx_tx.send(ProcessFeeTxCommand::Tx(trade_no.to_string()));
        Ok(())
    }

    pub(crate) async fn submit_confirm_report_tx(
        &self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        let _ =
            self.confirm_report_tx.send(ProcessFeeTxConfirmReportCommand::Tx(trade_no.to_string()));
        Ok(())
    }

    pub(crate) async fn close(&self) -> Result<(), ServiceError> {
        let _ = self.shutdown_tx.send(());
        if let Some(handle) = self.tx_handle.lock().await.take() {
            handle.await.map_err(|_| {
                ServiceError::System(crate::error::system::SystemError::BackendEndpointNotFound)
            })??;
        }
        if let Some(handle) = self.tx_report_handle.lock().await.take() {
            handle.await.map_err(|_| {
                ServiceError::System(crate::error::system::SystemError::BackendEndpointNotFound)
            })??;
        }
        if let Some(handle) = self.tx_confirm_report_handle.lock().await.take() {
            handle.await.map_err(|_| {
                ServiceError::System(crate::error::system::SystemError::BackendEndpointNotFound)
            })??;
        }
        Ok(())
    }
}

struct ProcessFeeTx {
    shutdown_rx: broadcast::Receiver<()>,
    tx_rx: mpsc::Receiver<ProcessFeeTxCommand>,
    report_tx: mpsc::Sender<ProcessFeeTxReportCommand>,
}

impl ProcessFeeTx {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        tx_rx: mpsc::Receiver<ProcessFeeTxCommand>,
        report_tx: mpsc::Sender<ProcessFeeTxReportCommand>,
    ) -> Self {
        Self { shutdown_rx, tx_rx, report_tx }
    }

    async fn run(&mut self) -> Result<(), ServiceError> {
        tracing::info!("starting process fee -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process fee tx -------------------------------");
                    break;
                }
                msg = self.tx_rx.recv() => {
                    if let Some(cmd) = msg {
                        match cmd {
                            ProcessFeeTxCommand::Tx(trade_no) => {
                                match self.process_fee_single_tx_by_trade_no(&trade_no).await {
                                    Ok(_) => {}
                                    Err(err) => {
                                        tracing::error!("failed to process fee tx: {}", err);
                                    }
                                }
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    match self.process_fee_tx().await {
                        Ok(_) => {}
                        Err(err) => {
                            tracing::error!("failed to process fee tx: {}", err);
                        }
                    }
                }
            }
        }
        tracing::info!("closing process fee tx ------------------------------- end");
        Ok(())
    }

    async fn process_fee_single_tx_by_trade_no(&self, trade_no: &str) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res =
            ApiFeeRepo::get_api_fee_by_trade_no_status(&pool, &trade_no, &[ApiFeeStatus::Init])
                .await;
        if res.is_ok() {
            self.process_fee_single_tx(res.unwrap()).await
        } else {
            Err(ServiceError::Business(ApiWalletError::OrderNotFound(trade_no.to_string()).into()))
        }
    }

    async fn process_fee_tx(&self) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // 获取交易这里有问题
        let (_, transfer_fees) =
            ApiFeeRepo::page_api_fee_with_status(&pool.clone(), 0, 1000, &[ApiFeeStatus::Init])
                .await?;
        for req in transfer_fees {
            self.process_fee_single_tx(req).await?;
        }
        Ok(())
    }

    async fn process_fee_single_tx(&self, req: ApiFeeEntity) -> Result<(), ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "process fee tx -------------------------------");
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

        let passwd = ApiWalletDomain::get_passwd().await?;
        let transfer_req = ApiTransferReq { base: params, password: passwd };

        // 发交易
        let tx_resp = ApiTransDomain::transfer(transfer_req).await;
        match tx_resp {
            Ok(tx) => self.handle_fee_tx_success(&req.trade_no, tx).await,
            Err(err) => {
                tracing::error!("failed to process fee tx: {}", err);
                self.handle_fee_tx_failed(&req.trade_no).await?;
                Err(err)
            }
        }
    }

    async fn handle_fee_tx_success(
        &self,
        trade_no: &str,
        tx: TransferResp,
    ) -> Result<(), ServiceError> {
        let resource_consume = if tx.consumer.is_none() {
            "0".to_string()
        } else {
            tx.consumer.unwrap().energy_used.to_string()
        };
        // 更新发送交易状态
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiFeeRepo::update_api_fee_tx_status(
            &pool,
            trade_no,
            &tx.tx_hash,
            &resource_consume,
            &tx.fee,
            ApiFeeStatus::SendingTx,
        )
        .await?;
        tracing::info!("send tx success ---");
        // 上报交易不影响交易偏移量计算
        let _ = self.report_tx.send(ProcessFeeTxReportCommand::Tx(trade_no.to_string()));
        Ok(())
    }

    async fn handle_fee_tx_failed(&self, trade_no: &str) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiFeeRepo::update_api_fee_status(&pool, trade_no, ApiFeeStatus::SendingTxFailed).await?;
        // 上报交易不影响交易偏移量计算
        let _ = self.report_tx.send(ProcessFeeTxReportCommand::Tx(trade_no.to_string()));
        Ok(())
    }
}

struct ProcessFeeTxReport {
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessFeeTxReportCommand>,
    failed_count: i64,
}

impl ProcessFeeTxReport {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessFeeTxReportCommand>,
    ) -> Self {
        Self { shutdown_rx, report_rx, failed_count: 0 }
    }

    async fn run(&mut self) -> Result<(), ServiceError> {
        tracing::info!("starting process fee tx report -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
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
                    )
                    .await?;
                } else {
                    ApiFeeRepo::update_api_fee_next_status(
                        &pool,
                        &req.trade_no,
                        ApiFeeStatus::SendingTx,
                        ApiFeeStatus::SendingTxReport,
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

struct ProcessFeeTxConfirmReport {
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessFeeTxConfirmReportCommand>,
    failed_count: i64,
}

impl ProcessFeeTxConfirmReport {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessFeeTxConfirmReportCommand>,
    ) -> Self {
        Self { shutdown_rx, report_rx, failed_count: 0 }
    }

    async fn run(&mut self) -> Result<(), ServiceError> {
        tracing::info!("starting process fee tx confirm report -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
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
                tracing::info!("process_fee_single_tx_confirm_report success");
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                ApiFeeRepo::update_api_fee_next_status(
                    &pool,
                    &req.trade_no,
                    req.status,
                    ApiFeeStatus::ReceivedConfirmReport,
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
