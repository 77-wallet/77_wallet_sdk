use crate::{
    domain::{api_wallet::fee::ApiFeeDomain, coin::CoinDomain},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use tokio::{
    sync::{Mutex, mpsc},
    task::JoinHandle,
};
use wallet_database::{
    entities::api_fee::{ApiFeeEntity, ApiFeeStatus},
    repositories::{api_fee::ApiFeeRepo, api_window::ApiWindowRepo},
};

pub(crate) enum ProcessFeeTxCommand {
    Tx,
    Close,
}

#[derive(Debug)]
pub(crate) struct ProcessFeeTxHandle {
    tx: mpsc::Sender<ProcessFeeTxCommand>,
    handle: Mutex<Option<JoinHandle<Result<(), crate::ServiceError>>>>,
}

impl ProcessFeeTxHandle {
    pub(crate) async fn new() -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let mut tx = ProcessWithdrawTx::new(shutdown_rx);
        let handle = tokio::spawn(async move { tx.run().await });
        Self { tx: shutdown_tx, handle: Mutex::new(Some(handle)) }
    }

    pub(crate) async fn submit_tx(&self, tx: ProcessFeeTxCommand) -> Result<(), anyhow::Error> {
        let _ = self.tx.send(tx).await;
        Ok(())
    }

    pub(crate) async fn close(&self) -> Result<(), crate::ServiceError> {
        let _ = self.tx.send(ProcessFeeTxCommand::Close).await;
        if let Some(handle) = self.handle.lock().await.take() {
            handle.await.map_err(|_| {
                crate::ServiceError::System(crate::SystemError::BackendEndpointNotFound)
            })??;
        }
        Ok(())
    }
}

struct ProcessWithdrawTx {
    rx: mpsc::Receiver<ProcessFeeTxCommand>,
}

impl ProcessWithdrawTx {
    fn new(rx: mpsc::Receiver<ProcessFeeTxCommand>) -> Self {
        Self { rx }
    }

    async fn run(&mut self) -> Result<(), crate::ServiceError> {
        tracing::info!("starting process fee -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    match msg {
                        Some(cmd) => {
                            match cmd {
                                ProcessFeeTxCommand::Tx => {
                                     match self.process_fee_tx().await {
                                        Ok(_) => {}
                                        Err(_) => {
                                            tracing::error!("failed to process fee tx");
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
                        _ => {}
                    }
                }
                _ = iv.tick() => {
                    match self.process_fee_tx().await {
                        Ok(_) => {}
                        Err(_) => {
                            tracing::error!("failed to process fee tx");
                        }
                    }
                }
            }
        }
        tracing::info!("closing process fee tx ------------------------------- end");
        Ok(())
    }

    async fn process_fee_tx(&self) -> Result<(), anyhow::Error> {
        tracing::info!("starting process fee -------------------------------1");
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let offset = ApiWindowRepo::get_api_offset(&pool.clone(), 2).await?;
        tracing::info!("starting process fee -------------------------------2");
        let (_, transfer_fees) =
            ApiFeeRepo::page_api_fee_with_status(&pool.clone(), offset, 1000, ApiFeeStatus::Init)
                .await?;
        let transfer_fees_len = transfer_fees.len();
        tracing::info!(transfer_fees=%transfer_fees.len(), "starting process fee -------------------------------3");
        for req in transfer_fees {
            self.process_fee_single_tx(req).await?;
        }
        ApiWindowRepo::upsert_api_offset(&pool, 2, offset + transfer_fees_len as i64).await?;
        Ok(())
    }

    async fn process_fee_single_tx(&self, req: ApiFeeEntity) -> Result<(), anyhow::Error> {
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
        let tx_resp = ApiFeeDomain::transfer(transfer_req).await;
        match tx_resp {
            Ok(tx) => {
                let resource_consume = if tx.consumer.is_none() {
                    "0".to_string()
                } else {
                    tx.consumer.unwrap().energy_used.to_string()
                };
                let pool = crate::manager::Context::get_global_sqlite_pool()?;
                ApiFeeRepo::update_api_fee_tx_status(
                    &pool,
                    &req.trade_no,
                    &tx.tx_hash,
                    &resource_consume,
                    &tx.fee,
                    ApiFeeStatus::SendingTx,
                )
                .await?;
            }
            Err(_) => {
                // 上报
                tracing::error!("failed to process fee tx ---");
                let pool = crate::manager::Context::get_global_sqlite_pool()?;
                ApiFeeRepo::update_api_fee_status(&pool, &req.trade_no, ApiFeeStatus::Failure)
                    .await?;
            }
        }
        Ok(())
    }
}
