use crate::{
    Context,
    domain::{api_wallet::withdraw::ApiWithdrawDomain, chain::TransferResp, coin::CoinDomain},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use tokio::{
    sync::{Mutex, broadcast},
    task::JoinHandle,
};
use wallet_database::{
    entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
    repositories::{api_window::ApiWindowRepo, api_withdraw::ApiWithdrawRepo},
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransStatus, TransType, TxExecReceiptUploadReq,
};

#[derive(Clone)]
pub(crate) enum ProcessWithdrawTxCommand {
    Tx,
    Close,
}

#[derive(Debug)]
pub(crate) struct ProcessWithdrawTxHandle {
    tx: broadcast::Sender<ProcessWithdrawTxCommand>,
    tx_handle: Mutex<Option<JoinHandle<Result<(), crate::ServiceError>>>>,
    tx_report_handle: Mutex<Option<JoinHandle<Result<(), crate::ServiceError>>>>,
}

impl ProcessWithdrawTxHandle {
    pub(crate) async fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let rx = shutdown_tx.subscribe();
        let rx_report = shutdown_tx.subscribe();
        let mut tx = ProcessWithdrawTx::new(rx);
        let handle = tokio::spawn(async move { tx.run().await });
        let mut tx_report = ProcessWithdrawTxReport::new(rx_report);
        let tx_report_handle = tokio::spawn(async move { tx_report.run().await });
        Self {
            tx: shutdown_tx,
            tx_handle: Mutex::new(Some(handle)),
            tx_report_handle: Mutex::new(Some(tx_report_handle)),
        }
    }

    pub(crate) async fn submit_tx(
        &self,
        tx: ProcessWithdrawTxCommand,
    ) -> Result<(), anyhow::Error> {
        let _ = self.tx.send(tx);
        Ok(())
    }

    pub(crate) async fn close(&self) -> Result<(), crate::ServiceError> {
        let _ = self.tx.send(ProcessWithdrawTxCommand::Close);
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
        Ok(())
    }
}

struct ProcessWithdrawTx {
    rx: broadcast::Receiver<ProcessWithdrawTxCommand>,
}

impl ProcessWithdrawTx {
    fn new(rx: broadcast::Receiver<ProcessWithdrawTxCommand>) -> Self {
        Self { rx }
    }

    async fn run(&mut self) -> Result<(), crate::ServiceError> {
        tracing::info!("starting process withdraw -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    match msg {
                        Ok(cmd) => {
                            match cmd {
                                ProcessWithdrawTxCommand::Tx => {
                                     match self.process_withdraw_tx().await {
                                        Ok(_) => {}
                                        Err(_) => {
                                            tracing::error!("failed to process withdraw tx");
                                        }
                                    }
                                    iv.reset();
                                }
                                ProcessWithdrawTxCommand::Close => {
                                    tracing::info!("closing process withdraw tx -------------------------------");
                                    break;
                                }
                            }
                        }
                        _ => {}
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

    async fn process_withdraw_tx(&self) -> Result<(), anyhow::Error> {
        tracing::info!("starting process withdraw -------------------------------1");
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let offset = ApiWindowRepo::get_api_offset(&pool.clone(), 1).await?;
        tracing::info!("starting process withdraw -------------------------------2");
        let (_, withdraws) = ApiWithdrawRepo::page_api_withdraw_with_status(
            &pool.clone(),
            offset,
            1000,
            ApiWithdrawStatus::AuditPass,
        )
        .await?;
        let withdraws_len = withdraws.len();
        tracing::info!(withdraws=%withdraws.len(), "starting process withdraw -------------------------------3");
        for req in withdraws {
            self.process_withdraw_single_tx(req).await?;
        }
        ApiWindowRepo::upsert_api_offset(&pool, 1, offset + withdraws_len as i64).await?;
        Ok(())
    }

    async fn process_withdraw_single_tx(
        &self,
        req: ApiWithdrawEntity,
    ) -> Result<(), anyhow::Error> {
        tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "---------------------------------4");

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
        let tx_resp = ApiWithdrawDomain::transfer(transfer_req).await;
        match tx_resp {
            Ok(tx) => {
                self.handle_withdraw_tx_success(&req.trade_no, tx).await?;
            }
            Err(_) => {
                self.handle_withdraw_tx_failed(&req.trade_no).await?;
            }
        }
        Ok(())
    }

    async fn handle_withdraw_tx_success(
        &self,
        trade_no: &str,
        tx: TransferResp,
    ) -> Result<(), anyhow::Error> {
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
        let backend_api = Context::get_global_backend_api()?;
        let _ = backend_api
            .upload_tx_exec_receipt(&TxExecReceiptUploadReq::new(
                trade_no,
                TransType::Wd,
                &tx.tx_hash,
                TransStatus::Success,
                "",
            ))
            .await?;
        ApiWithdrawRepo::update_api_withdraw_next_status(
            &pool,
            trade_no,
            ApiWithdrawStatus::SendingTx,
            ApiWithdrawStatus::ReceivedTxReport,
        )
        .await?;
        Ok(())
    }

    async fn handle_withdraw_tx_failed(&self, trade_no: &str) -> Result<(), anyhow::Error> {
        // 更新交易状态,发送失败
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status(
            &pool,
            trade_no,
            ApiWithdrawStatus::SendingTxFailed,
        )
        .await?;
        // 上报交易
        let backend_api = Context::get_global_backend_api()?;
        let _ = backend_api
            .upload_tx_exec_receipt(&TxExecReceiptUploadReq::new(trade_no, TransType::Wd, "", TransStatus::Fail, ""))
            .await?;
        ApiWithdrawRepo::update_api_withdraw_next_status(
            &pool,
            trade_no,
            ApiWithdrawStatus::SendingTxFailed,
            ApiWithdrawStatus::Failure,
        )
        .await?;
        Ok(())
    }
}

struct ProcessWithdrawTxReport {
    rx: broadcast::Receiver<ProcessWithdrawTxCommand>,
}

impl ProcessWithdrawTxReport {
    fn new(rx: broadcast::Receiver<ProcessWithdrawTxCommand>) -> Self {
        Self { rx }
    }

    async fn run(&mut self) -> Result<(), crate::ServiceError> {
        tracing::info!("starting process withdraw tx report -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    match msg {
                        Ok(cmd) => {
                            match cmd {
                                ProcessWithdrawTxCommand::Tx => {
                                    match self.process_withdraw_tx_report().await {
                                        Ok(_) => {},
                                        Err(_) => {
                                            tracing::error!("failed to process withdraw tx report");
                                        }
                                    }
                                    iv.reset();
                                }
                                ProcessWithdrawTxCommand::Close => {
                                    tracing::info!("closing process withdraw tx report -------------------------------");
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::info!("channel closed, exiting process withdraw tx report loop");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            tracing::warn!("lagged behind on withdraw tx report commands");
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

    async fn process_withdraw_tx_report(&self) -> Result<(), anyhow::Error> {
        tracing::info!("starting process withdraw tx report -------------------------------");
        Ok(())
    }

    async fn process_withdraw_single_tx_report(
        &self,
        req: ApiWithdrawEntity,
    ) -> Result<(), anyhow::Error> {
        tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "---------------------------------4");
        Ok(())
    }
}
