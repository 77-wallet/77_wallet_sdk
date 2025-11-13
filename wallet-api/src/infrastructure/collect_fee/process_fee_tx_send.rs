use crate::{
    context::Context,
    domain::{
        api_wallet::{trans::ApiTransDomain, wallet::ApiWalletDomain},
        chain::TransferResp,
        coin::CoinDomain,
    },
    error::service::ServiceError,
    infrastructure::collect_fee::command::{ProcessFeeTxCommand, ProcessFeeTxReportCommand},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use rust_decimal::Decimal;
use std::{str::FromStr, sync::Arc};
use tokio::{
    sync::{broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    entities::{
        api_fee::{ApiFeeEntity, ApiFeeStatus},
        api_withdraw::ApiWithdrawStatus,
    },
    repositories::api_wallet::{fee::ApiFeeRepo, nonce::ApiNonceRepo, withdraw::ApiWithdrawRepo},
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_types::chain::chain::ChainCode;

pub(super) struct ProcessFeeTx {
    ctx: &'static Context,
    pool: Arc<sqlx::SqlitePool>,
    shutdown_rx: broadcast::Receiver<()>,
    tx_rx: mpsc::Receiver<ProcessFeeTxCommand>,
    report_tx: mpsc::Sender<ProcessFeeTxReportCommand>,
}

impl ProcessFeeTx {
    pub(super) fn new(
        ctx: &'static Context,
        pool: Arc<sqlx::SqlitePool>,
        shutdown_rx: broadcast::Receiver<()>,
        tx_rx: mpsc::Receiver<ProcessFeeTxCommand>,
        report_tx: mpsc::Sender<ProcessFeeTxReportCommand>,
    ) -> Self {
        Self { ctx, pool, shutdown_rx, tx_rx, report_tx }
    }

    pub(super) async fn run(&mut self) {
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
                                self.process_fee_single_tx_by_trade_no(&trade_no).await;
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    self.process_fee_tx().await
                }
            }
        }
        tracing::info!("closing process fee tx ------------------------------- end");
    }

    async fn process_fee_single_tx_by_trade_no(&self, trade_no: &str) {
        let res = ApiFeeRepo::get_api_fee_by_trade_no_status(
            &self.pool,
            &trade_no,
            &[ApiFeeStatus::Init],
        )
        .await;
        match res {
            Ok(fee) => self.process_fee_single_tx(fee).await,
            Err(err) => {
                tracing::error!("failed to process transfer fee tx: {:?}", err);
            }
        }
    }

    async fn process_fee_tx(&self) {
        // 获取交易这里有问题
        let res =
            ApiFeeRepo::page_api_fee_with_status(&self.pool, 0, 1000, &[ApiFeeStatus::Init]).await;
        match res {
            Ok((_, transfer_fees)) => {
                for req in transfer_fees {
                    self.process_fee_single_tx(req).await;
                }
            }
            Err(err) => {
                tracing::error!("failed to process transfer fee tx: {:?}", err);
            }
        }
    }

    async fn process_fee_single_tx(&self, req: ApiFeeEntity) {
        tracing::info!(trade_no=%req.trade_no, "process fee tx -------------------------------");
        // check
        if !self.check_digest(&req).await {
            return self
                .handle_fee_tx_failed(
                    &req.trade_no,
                    ServiceError::Parameter("validate failed".to_string()),
                )
                .await;
        }

        let from_addr = req.from_addr.clone();
        self.ctx.lock_account(&from_addr).await;
        let transfer_req_res = self.gen_transfer_req(&req).await;
        match transfer_req_res {
            Ok(transfer_req) => {
                // 发交易
                let nonce = transfer_req.nonce;
                let tx_resp = ApiTransDomain::transfer(transfer_req).await;
                match tx_resp {
                    Ok(tx) => self.handle_fee_tx_success(req, tx, nonce).await,
                    Err(err) => {
                        tracing::error!("failed to process fee tx: {}", err);
                        self.handle_fee_tx_failed(&req.trade_no, err).await
                    }
                }
            }
            Err(err) => self.handle_fee_tx_failed(&req.trade_no, err).await,
        }
        self.ctx.unlock_account(&from_addr).await;
    }

    async fn check_digest(&self, req: &ApiFeeEntity) -> bool {
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let mut d = Decimal::from_str(req.value.as_str()).unwrap();
        d = d.normalize();
        let raw_data = req.from_addr.clone() + req.to_addr.as_str() + d.to_string().as_str() + sn;
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(&raw_data));
        req.validate == digest
    }

    async fn get_eth_nonce(&self, from_addr: &str, chain_code: &str) -> i64 {
        match ApiNonceRepo::get_api_nonce(&self.pool, from_addr, chain_code).await {
            Ok(nonce) => nonce + 1,
            Err(_) => 0,
        }
    }

    async fn gen_transfer_req(&self, req: &ApiFeeEntity) -> Result<ApiTransferReq, ServiceError> {
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

        let chain_code = req.chain_code.as_str();
        let chain_code: ChainCode = chain_code.try_into()?;
        let nonce: i64 = match chain_code {
            ChainCode::Tron => 0,
            ChainCode::Bitcoin => 0,
            ChainCode::Solana => 0,
            ChainCode::Ethereum => self.get_eth_nonce(&req.from_addr, &req.chain_code).await,
            ChainCode::BnbSmartChain => self.get_eth_nonce(&req.from_addr, &req.chain_code).await,
            ChainCode::Litecoin => 0,
            ChainCode::Dogcoin => 0,
            ChainCode::Sui => 0,
            ChainCode::Ton => 0,
        };
        Ok(ApiTransferReq { base: params, password: passwd, nonce: nonce as u64 })
    }

    async fn handle_fee_tx_success(&self, req: ApiFeeEntity, tx: TransferResp, nonce: u64) {
        let resource_consume = if tx.consumer.is_none() {
            "0".to_string()
        } else {
            tx.consumer.unwrap().energy_used.to_string()
        };
        let res = if req.chain_code == ChainCode::Ethereum.to_string()
            || req.chain_code == ChainCode::BnbSmartChain.to_string()
        {
            ApiFeeRepo::update_api_fee_tx_status_nonce(
                &self.pool,
                &req.from_addr,
                &req.chain_code,
                &req.trade_no,
                nonce as i64,
                &tx.tx_hash,
                &resource_consume,
                &tx.fee,
                ApiFeeStatus::SendingTx,
            )
            .await
        } else {
            // 更新发送交易状态
            ApiFeeRepo::update_api_fee_tx_status(
                &self.pool,
                &req.trade_no,
                &tx.tx_hash,
                &resource_consume,
                &tx.fee,
                ApiFeeStatus::SendingTx,
            )
            .await
        };

        match res {
            Ok(_) => {
                tracing::info!("send tx success ---");
                // 上报交易不影响交易偏移量计算
                let _ =
                    self.report_tx.send(ProcessFeeTxReportCommand::Tx(req.trade_no.to_string()));
            }
            Err(err) => {
                tracing::error!("handle_fee_tx_success: {}", err)
            }
        }
    }

    async fn handle_fee_tx_failed(&self, trade_no: &str, err: ServiceError) {
        let res = ApiFeeRepo::update_api_fee_status_and_err(
            &self.pool,
            trade_no,
            ApiFeeStatus::SendingTxFailed,
            &err.to_string(),
        )
        .await;
        match res {
            Ok(_) => {
                // 上报交易不影响交易偏移量计算
                let _ = self.report_tx.send(ProcessFeeTxReportCommand::Tx(trade_no.to_string()));
            }
            Err(err) => {
                tracing::error!("handle_fee_tx_failed: {}", err)
            }
        }
    }
}
