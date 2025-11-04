use crate::{
    domain::{
        api_wallet::{trans::ApiTransDomain, wallet::ApiWalletDomain},
        chain::TransferResp,
        coin::CoinDomain,
    },
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    infrastructure::collect_fee::command::{ProcessFeeTxCommand, ProcessFeeTxReportCommand},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::{
    sync::{broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    entities::api_fee::{ApiFeeEntity, ApiFeeStatus},
    repositories::api_wallet::fee::ApiFeeRepo,
};
use wallet_ecdh::GLOBAL_KEY;

pub(super) struct ProcessFeeTx {
    shutdown_rx: broadcast::Receiver<()>,
    tx_rx: mpsc::Receiver<ProcessFeeTxCommand>,
    report_tx: mpsc::Sender<ProcessFeeTxReportCommand>,
}

impl ProcessFeeTx {
    pub(super) fn new(
        shutdown_rx: broadcast::Receiver<()>,
        tx_rx: mpsc::Receiver<ProcessFeeTxCommand>,
        report_tx: mpsc::Sender<ProcessFeeTxReportCommand>,
    ) -> Self {
        Self { shutdown_rx, tx_rx, report_tx }
    }

    pub(super) async fn run(&mut self) -> Result<(), ServiceError> {
        tracing::info!("starting process fee -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
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
        // check
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let mut d = Decimal::from_str(req.value.as_str()).unwrap();
        d = d.normalize();
        let raw_data = req.from_addr.clone() + req.to_addr.as_str() + d.to_string().as_str() + sn;
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(&raw_data));
        if req.validate != digest {
            return self
                .handle_fee_tx_failed(
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
            Ok(tx) => self.handle_fee_tx_success(&req.trade_no, tx).await,
            Err(err) => {
                tracing::error!("failed to process fee tx: {}", err);
                self.handle_fee_tx_failed(&req.trade_no, err).await
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

    async fn handle_fee_tx_failed(
        &self,
        trade_no: &str,
        err: ServiceError,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiFeeRepo::update_api_fee_status(
            &pool,
            trade_no,
            ApiFeeStatus::SendingTxFailed,
            &err.to_string(),
        )
        .await?;
        // 上报交易不影响交易偏移量计算
        let _ = self.report_tx.send(ProcessFeeTxReportCommand::Tx(trade_no.to_string()));
        Ok(())
    }
}
