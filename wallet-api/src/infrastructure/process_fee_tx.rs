use crate::{
    Context,
    domain::{api_wallet::fee::ApiFeeDomain, chain::TransferResp, coin::CoinDomain},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use tokio::{
    sync::{Mutex, broadcast},
    task::JoinHandle,
};
use wallet_database::{
    entities::api_fee::{ApiFeeEntity, ApiFeeStatus},
    repositories::{api_fee::ApiFeeRepo, api_window::ApiWindowRepo},
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransStatus, TransType, TxExecReceiptUploadReq,
};

#[derive(Clone)]
pub(crate) enum ProcessFeeTxCommand {
    Tx,
    Close,
}

#[derive(Debug)]
pub(crate) struct ProcessFeeTxHandle {
    tx: broadcast::Sender<ProcessFeeTxCommand>,
    tx_handle: Mutex<Option<JoinHandle<Result<(), crate::ServiceError>>>>,
    tx_report_handle: Mutex<Option<JoinHandle<Result<(), crate::ServiceError>>>>,
}

impl ProcessFeeTxHandle {
    pub(crate) async fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let rx1 = shutdown_tx.subscribe();
        let rx2 = shutdown_tx.subscribe();
        let mut tx = ProcessWithdrawTx::new(rx1);
        let tx_handle = tokio::spawn(async move { tx.run().await });
        let mut tx_report = ProcessWithdrawTxReport::new(rx2);
        let tx_report_handle = tokio::spawn(async move { tx_report.run().await });
        Self {
            tx: shutdown_tx,
            tx_handle: Mutex::new(Some(tx_handle)),
            tx_report_handle: Mutex::new(Some(tx_report_handle)),
        }
    }

    pub(crate) async fn submit_tx(&self, tx: ProcessFeeTxCommand) -> Result<(), anyhow::Error> {
        let _ = self.tx.send(tx);
        Ok(())
    }

    pub(crate) async fn close(&self) -> Result<(), crate::ServiceError> {
        let _ = self.tx.send(ProcessFeeTxCommand::Close);
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
    rx: broadcast::Receiver<ProcessFeeTxCommand>,
}

impl ProcessWithdrawTx {
    fn new(rx: broadcast::Receiver<ProcessFeeTxCommand>) -> Self {
        Self { rx }
    }

    async fn run(&mut self) -> Result<(), crate::ServiceError> {
        tracing::info!("starting process fee -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    match msg {
                        Ok(cmd) => {
                            match cmd {
                                ProcessFeeTxCommand::Tx => {
                                     match self.process_fee_tx().await {
                                        Ok(_) => {}
                                        Err(err) => {
                                            tracing::error!("failed to process fee tx: {}", err);
                                        }
                                    }
                                    iv.reset();
                                }
                                ProcessFeeTxCommand::Close => {
                                    tracing::info!("closing process fee tx -------------------------------");
                                    break;
                                }
                            }
                        }
                        Err(_) => {}
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

    async fn process_fee_tx(&self) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let offset = ApiWindowRepo::get_api_offset(&pool.clone(), 2).await?;
        let (_, transfer_fees) =
            ApiFeeRepo::page_api_fee_with_status(&pool.clone(), offset, 1000, &[ApiFeeStatus::Init])
                .await?;
        let transfer_fees_len = transfer_fees.len();
        for req in transfer_fees {
            self.process_fee_single_tx(req).await?;
        }
        ApiWindowRepo::upsert_api_offset(&pool, 2, offset + transfer_fees_len as i64).await?;
        Ok(())
    }

    async fn process_fee_single_tx(&self, req: ApiFeeEntity) -> Result<(), crate::ServiceError> {
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
        let tx_resp = ApiFeeDomain::transfer(transfer_req).await;
        match tx_resp {
            Ok(tx) => self.handle_fee_tx_success(&req.trade_no, tx).await,
            Err(_) => self.handle_fee_tx_failed(&req.trade_no).await,
        }
    }

    async fn handle_fee_tx_success(
        &self,
        trade_no: &str,
        tx: TransferResp,
    ) -> Result<(), crate::ServiceError> {
        let resource_consume = if tx.consumer.is_none() {
            "0".to_string()
        } else {
            tx.consumer.unwrap().energy_used.to_string()
        };
        // 更新发送交易状态
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiFeeRepo::update_api_fee_tx_status(
            &pool,
            trade_no,
            &tx.tx_hash,
            &resource_consume,
            &tx.fee,
            ApiFeeStatus::SendingTx,
        )
        .await?;
        let backend_api = Context::get_global_backend_api()?;
        let _ = backend_api
            .upload_tx_exec_receipt(&TxExecReceiptUploadReq::new(
                trade_no,
                TransType::Fee,
                &tx.tx_hash,
                TransStatus::Success,
                "",
            ))
            .await?;
        ApiFeeRepo::update_api_fee_next_status(
            &pool,
            &trade_no,
            ApiFeeStatus::SendingTx,
            ApiFeeStatus::ReceivedTxReport,
        )
        .await?;
        tracing::info!("upload tx exec receipt success ---");
        Ok(())
    }

    async fn handle_fee_tx_failed(&self, trade_no: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiFeeRepo::update_api_fee_status(&pool, trade_no, ApiFeeStatus::SendingTxFailed).await?;
        let backend_api = Context::get_global_backend_api()?;
        let _ = backend_api
            .upload_tx_exec_receipt(&TxExecReceiptUploadReq::new(
                trade_no,
                TransType::Fee,
                "",
                TransStatus::Fail,
                "",
            ))
            .await?;
        ApiFeeRepo::update_api_fee_next_status(
            &pool,
            &trade_no,
            ApiFeeStatus::SendingTxFailed,
            ApiFeeStatus::ReceivedTxReport,
        )
        .await?;
        Ok(())
    }
}

struct ProcessWithdrawTxReport {
    rx: broadcast::Receiver<ProcessFeeTxCommand>,
}

impl ProcessWithdrawTxReport {
    fn new(rx: broadcast::Receiver<ProcessFeeTxCommand>) -> Self {
        Self { rx }
    }

    async fn run(&mut self) -> Result<(), crate::ServiceError> {
        tracing::info!("starting process fee tx report -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    match msg {
                        Ok(cmd) => {
                            match cmd {
                                ProcessFeeTxCommand::Tx => {
                                     match self.process_fee_tx_report().await {
                                        Ok(_) => {}
                                        Err(err) => {
                                            tracing::error!("failed to process fee tx report: {}", err);
                                        }
                                    }
                                    iv.reset();
                                }
                                ProcessFeeTxCommand::Close => {
                                    tracing::info!("closing process fee tx report -------------------------------");
                                    break;
                                }
                            }
                        }
                        Err(_) => {}
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

    async fn process_fee_tx_report(&self) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let offset = ApiWindowRepo::get_api_offset(&pool.clone(), 20).await?;
        let (_, transfer_fees) =
            ApiFeeRepo::page_api_fee_with_status(&pool, offset, 1000, &[ApiFeeStatus::SendingTx, ApiFeeStatus::SendingTxFailed])
                .await?;
        let transfer_fees_len = transfer_fees.len();
        for req in transfer_fees {
            self.process_fee_single_tx_report(req).await?;
        }
        ApiWindowRepo::upsert_api_offset(&pool, 20, offset + transfer_fees_len as i64).await?;
        Ok(())
    }

    async fn process_fee_single_tx_report(
        &self,
        req: ApiFeeEntity,
    ) -> Result<(), crate::ServiceError> {
        let status = if req.status == ApiFeeStatus::SendingTxFailed { TransStatus::Fail } else { TransStatus::Success };
        let backend_api = Context::get_global_backend_api()?;
        let _ = backend_api
            .upload_tx_exec_receipt(&TxExecReceiptUploadReq::new(
                &req.trade_no,
                TransType::Fee,
                &req.tx_hash,
                status,
                &req.notes,
            ))
            .await?;
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiFeeRepo::update_api_fee_next_status(
            &pool,
            &req.trade_no,
            ApiFeeStatus::SendingTx,
            ApiFeeStatus::ReceivedTxReport,
        )
        .await?;
        tracing::info!("upload tx exec receipt success ---");
        Ok(())
    }
}
