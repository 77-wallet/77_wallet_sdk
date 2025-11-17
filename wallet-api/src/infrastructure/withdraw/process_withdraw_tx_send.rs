use crate::{
    context::Context,
    domain::{
        api_wallet::{coin::ApiCoinDomain, trans::ApiTransDomain, wallet::ApiWalletDomain},
        chain::TransferResp,
    },
    error::service::ServiceError,
    infrastructure::withdraw::command::{ProcessWithdrawTxCommand, ProcessWithdrawTxReportCommand},
    messaging::notify::{FrontendNotifyEvent, api_wallet::WithdrawFront, event::NotifyEvent},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use rust_decimal::Decimal;
use std::{str::FromStr, sync::Arc};
use tokio::{
    sync::{broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
    repositories::api_wallet::{nonce::ApiNonceRepo, withdraw::ApiWithdrawRepo},
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_types::chain::chain::ChainCode;

pub(super) struct ProcessWithdrawTx {
    ctx: &'static Context,
    pool: Arc<sqlx::SqlitePool>,
    shutdown_rx: broadcast::Receiver<()>,
    tx_rx: mpsc::Receiver<ProcessWithdrawTxCommand>,
    report_tx: mpsc::Sender<ProcessWithdrawTxReportCommand>,
}

impl ProcessWithdrawTx {
    pub(super) fn new(
        ctx: &'static Context,
        pool: Arc<sqlx::SqlitePool>,
        shutdown_rx: broadcast::Receiver<()>,
        tx_rx: mpsc::Receiver<ProcessWithdrawTxCommand>,
        report_tx: mpsc::Sender<ProcessWithdrawTxReportCommand>,
    ) -> Self {
        Self { ctx, pool, shutdown_rx, tx_rx, report_tx }
    }

    pub(super) async fn run(&mut self) {
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
                                self.process_withdraw_single_tx_by_id(&trade_no).await;
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    self.process_withdraw_tx().await
                }
            }
        }
        tracing::info!("closing process withdraw tx ------------------------------- end");
    }

    async fn process_withdraw_single_tx_by_id(&self, trade_no: &str) {
        let res = ApiWithdrawRepo::get_api_withdraw_by_trade_no_status(
            &self.pool,
            &trade_no,
            &[ApiWithdrawStatus::AuditPass],
        )
        .await;
        match res {
            Ok(res) => {
                self.process_withdraw_single_tx(res).await;
            }
            Err(err) => {
                tracing::warn!("process withdraw single tx by id: {:?}", err);
            }
        }
    }

    async fn process_withdraw_tx(&self) {
        let res = ApiWithdrawRepo::list_api_withdraw_with_status(
            &self.pool.clone(),
            vec![ApiWithdrawStatus::AuditPass],
            0,
            1000,
        )
        .await;
        match res {
            Ok(res) => {
                for req in res {
                    self.process_withdraw_single_tx(req).await;
                }
            }
            Err(err) => {
                tracing::warn!("process withdraw tx by id: {:?}", err);
            }
        }
    }

    async fn process_withdraw_single_tx(&self, req: ApiWithdrawEntity) {
        tracing::info!(trade_no=%req.trade_no,hash=%req.tx_hash,status=%req.status, "process_withdraw_single_tx ---------------------------------4");

        // check
        if !self.check_digest(&req).await {
            return self
                .handle_withdraw_tx_failed(
                    req,
                    ServiceError::Parameter("validate failed".to_string()),
                )
                .await;
        }

        // transfer
        let from_addr = req.from_addr.clone();
        self.ctx.lock_account(&from_addr).await;
        let transfer_req_res = self.gen_transfer_req(&req).await;
        match transfer_req_res {
            Ok(transfer_req) => {
                // 发交易
                let nonce = transfer_req.nonce;
                let tx_resp = ApiTransDomain::transfer(transfer_req).await;
                match tx_resp {
                    Ok(tx) => self.handle_withdraw_tx_success(req, tx, nonce).await,
                    Err(err) => {
                        tracing::error!(trade_no=%req.trade_no, "failed to process withdraw transfer tx: {}", err);
                        self.handle_withdraw_tx_failed(req, err).await
                    }
                }
            }
            Err(err) => {
                self.handle_withdraw_tx_failed(req, err).await;
            }
        }
        self.ctx.unlock_account(&from_addr).await;
    }

    async fn check_digest(&self, req: &ApiWithdrawEntity) -> bool {
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

    async fn gen_transfer_req(
        &self,
        req: &ApiWithdrawEntity,
    ) -> Result<ApiTransferReq, ServiceError> {
        let coin =
            ApiCoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_addr.clone()).await?;

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

    async fn handle_withdraw_tx_success(
        &self,
        req: ApiWithdrawEntity,
        tx: TransferResp,
        nonce: u64,
    ) {
        let data = NotifyEvent::Withdraw(WithdrawFront {
            uid: req.uid.to_string(),
            from_addr: req.from_addr.to_string(),
            to_addr: req.to_addr.to_string(),
            value: req.value.to_string(),
        });
        _ = FrontendNotifyEvent::new(data).send().await;
        let resource_consume = tx.resource_consume().unwrap_or_else(|_| "".to_string());
        // 更新交易状态
        let res = if req.chain_code == ChainCode::Ethereum.to_string()
            || req.chain_code == ChainCode::BnbSmartChain.to_string()
        {
            ApiWithdrawRepo::update_api_withdraw_tx_status_nonce(
                &self.pool,
                &req.from_addr,
                &req.chain_code,
                &req.trade_no,
                nonce as i64,
                &tx.tx_hash,
                &resource_consume,
                &tx.fee,
                None,
                "",
                ApiWithdrawStatus::SendingTx,
            )
            .await
        } else {
            ApiWithdrawRepo::update_api_withdraw_tx_status(
                &self.pool,
                &req.trade_no,
                req.nonce,
                &tx.tx_hash,
                &resource_consume,
                &tx.fee,
                None,
                "",
                ApiWithdrawStatus::SendingTx,
            )
            .await
        };
        match res {
            Ok(res) => {
                // 上报交易
                if (res != 1) {
                    tracing::error!(trade_no=%req.trade_no, "failed to process withdraw tx: {:?}", res);
                }
                let _ = self
                    .report_tx
                    .send(ProcessWithdrawTxReportCommand::Tx(req.trade_no.to_string()));
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "failed to process withdraw tx: {:?}", err);
            }
        }
    }

    async fn handle_withdraw_tx_failed(&self, req: ApiWithdrawEntity, err: ServiceError) {
        let data = NotifyEvent::Withdraw(WithdrawFront {
            uid: req.uid.to_string(),
            from_addr: req.from_addr.to_string(),
            to_addr: req.to_addr.to_string(),
            value: req.value.to_string(),
        });
        _ = FrontendNotifyEvent::new(data).send().await;
        // 更新交易状态,发送失败
        let res = ApiWithdrawRepo::update_api_withdraw_status_and_err(
            &self.pool,
            &req.trade_no,
            ApiWithdrawStatus::SendingTxFailed,
            101,
            &err.to_string(),
        )
        .await;
        match res {
            Ok(res) => {
                // 上报交易
                let _ = self
                    .report_tx
                    .send(ProcessWithdrawTxReportCommand::Tx(req.trade_no.to_string()));
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "failed to process withdraw tx: {:?}", err);
            }
        }
    }
}
