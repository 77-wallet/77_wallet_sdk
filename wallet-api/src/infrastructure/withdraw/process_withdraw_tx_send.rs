use crate::{
    domain::{
        api_wallet::{trans::ApiTransDomain, wallet::ApiWalletDomain},
        chain::TransferResp,
        coin::CoinDomain,
    },
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    infrastructure::withdraw::command::{ProcessWithdrawTxCommand, ProcessWithdrawTxReportCommand},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::{
    sync::{broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
    repositories::api_wallet::withdraw::ApiWithdrawRepo,
};
use wallet_ecdh::GLOBAL_KEY;

pub(super) struct ProcessWithdrawTx {
    shutdown_rx: broadcast::Receiver<()>,
    tx_rx: mpsc::Receiver<ProcessWithdrawTxCommand>,
    report_tx: mpsc::Sender<ProcessWithdrawTxReportCommand>,
}

impl ProcessWithdrawTx {
    pub(super) fn new(
        shutdown_rx: broadcast::Receiver<()>,
        tx_rx: mpsc::Receiver<ProcessWithdrawTxCommand>,
        report_tx: mpsc::Sender<ProcessWithdrawTxReportCommand>,
    ) -> Self {
        Self { shutdown_rx, tx_rx, report_tx }
    }

    pub(super) async fn run(&mut self) -> Result<(), ServiceError> {
        tracing::info!("starting process withdraw -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process withdraw tx -------------------------------");
                    break;
                }
                msg = self.tx_rx.recv() => {
                    if let Some(cmd) = msg {
                        match cmd {
                            ProcessWithdrawTxCommand::Tx(trade_no) => {
                                match self.process_withdraw_single_tx_by_id(&trade_no).await {
                                    Ok(_) => {}
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
                    match self.process_withdraw_tx().await {
                        Ok(_) => {}
                        Err(err) => {
                            tracing::error!("failed to process withdraw tx: {}", err);
                        }
                    }
                }
            }
        }
        tracing::info!("closing process withdraw tx ------------------------------- end");
        Ok(())
    }

    async fn process_withdraw_single_tx_by_id(&self, trade_no: &str) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiWithdrawRepo::get_api_withdraw_by_trade_no_status(
            &pool,
            &trade_no,
            &[ApiWithdrawStatus::AuditPass],
        )
        .await;
        tracing::info!("process withdraw single tx by id: {:?}", res);
        if res.is_ok() {
            self.process_withdraw_single_tx(res.unwrap()).await?;
            Ok(())
        } else {
            Err(ServiceError::Business(ApiWalletError::OrderNotFound(trade_no.to_string()).into()))
        }
    }

    async fn process_withdraw_tx(&self) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiWithdrawRepo::list_api_withdraw_with_status(
            &pool.clone(),
            vec![ApiWithdrawStatus::AuditPass],
            0,
            1000,
        )
        .await?;
        // tracing::info!("process withdraw single tx by id: {:?}", res);
        for req in res {
            self.process_withdraw_single_tx(req).await?;
        }
        Ok(())
    }

    async fn process_withdraw_single_tx(
        &self,
        req: ApiWithdrawEntity,
    ) -> Result<i32, ServiceError> {
        tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_withdraw_single_tx ---------------------------------4");

        // check
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let mut d = Decimal::from_str(req.value.as_str()).unwrap();
        d = d.normalize();
        let raw_data = req.from_addr.clone() + req.to_addr.as_str() + d.to_string().as_str() + sn;
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(&raw_data));
        if req.validate != digest {
            tracing::error!(raw_data=&raw_data,digest=%digest, "failed to process withdraw tx");
            return self
                .handle_withdraw_tx_failed(
                    &req.trade_no,
                    ServiceError::Parameter("validate failed".to_string()),
                )
                .await;
        }

        let coin =
            CoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_addr.clone()).await?;

        let mut params =
            ApiBaseTransferReq::new(&req.from_addr, &req.to_addr, &req.value, &req.chain_code);
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
            Ok(tx) => self.handle_withdraw_tx_success(&req.trade_no, tx).await,
            Err(err) => {
                tracing::warn!("failed to process withdraw tx: {}", err);
                self.handle_withdraw_tx_failed(&req.trade_no, err).await
            }
        }
    }

    async fn handle_withdraw_tx_success(
        &self,
        trade_no: &str,
        tx: TransferResp,
    ) -> Result<i32, ServiceError> {
        let resource_consume = if tx.consumer.is_none() {
            "0".to_string()
        } else {
            tx.consumer.unwrap().energy_used.to_string()
        };
        // 更新交易状态
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_tx_status(
            &pool,
            trade_no,
            &tx.tx_hash,
            &resource_consume,
            &tx.fee,
            None,
            "",
            ApiWithdrawStatus::SendingTx,
        )
        .await?;

        // 上报交易
        let _ = self.report_tx.send(ProcessWithdrawTxReportCommand::Tx(trade_no.to_string()));
        Ok(1)
    }

    async fn handle_withdraw_tx_failed(
        &self,
        trade_no: &str,
        err: ServiceError,
    ) -> Result<i32, ServiceError> {
        // 更新交易状态,发送失败
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status(
            &pool,
            trade_no,
            ApiWithdrawStatus::SendingTxFailed,
            &err.to_string(),
        )
        .await?;
        // 上报交易
        let _ = self.report_tx.send(ProcessWithdrawTxReportCommand::Tx(trade_no.to_string()));
        Ok(1)
    }
}
