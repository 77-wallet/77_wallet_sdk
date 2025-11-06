use crate::{
    domain::{
        api_wallet::{
            adapter_factory::ApiChainAdapterFactory, trans::ApiTransDomain, wallet::ApiWalletDomain,
        },
        chain::{TransferResp, transaction::ChainTransDomain},
        coin::CoinDomain,
    },
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    infrastructure::collect::command::{ProcessCollectTxCommand, ProcessCollectTxReportCommand},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use rust_decimal::Decimal;
use std::{str::FromStr, sync::Arc};
use tokio::{
    sync::{broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::collect::ApiCollectRepo,
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::{
    strategy::ChainConfig, transaction::ServiceFeeUploadReq,
};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::{conversion, unit};

pub(super) struct ProcessCollectTx {
    pool: Arc<sqlx::SqlitePool>,
    shutdown_rx: broadcast::Receiver<()>,
    tx_rx: mpsc::Receiver<ProcessCollectTxCommand>,
    report_tx: mpsc::Sender<ProcessCollectTxReportCommand>,
}

impl ProcessCollectTx {
    pub(super) fn new(
        pool: Arc<sqlx::SqlitePool>,
        shutdown_rx: broadcast::Receiver<()>,
        tx_rx: mpsc::Receiver<ProcessCollectTxCommand>,
        report_tx: mpsc::Sender<ProcessCollectTxReportCommand>,
    ) -> Self {
        Self { pool, shutdown_rx, tx_rx, report_tx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!("starting process collect -------------------------------");
        self.run_with_err().await;
        tracing::info!("closing process collect tx ------------------------------- end");
    }

    async fn run_with_err(&mut self) {
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process collect tx -------------------------------");
                    break;
                }
                msg = self.tx_rx.recv() => {
                    if let Some(cmd) = msg {
                        match cmd {
                            ProcessCollectTxCommand::Tx(trade_no) => {
                                self.process_collect_single_tx_by_trade_no(&trade_no).await;
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    self.process_collect_tx().await
                }
            }
        }
    }

    async fn process_collect_single_tx_by_trade_no(&self, trade_no: &str) {
        let res = ApiCollectRepo::get_api_collect_by_trade_no_status(
            &self.pool,
            &trade_no,
            &[ApiCollectStatus::Init],
        )
        .await;
        match res {
            Ok(res) => {
                self.process_collect_single_tx(res).await;
            }
            Err(err) => {
                tracing::warn!(trade_no=%trade_no, "process collect tx not found: {}", err);
            }
        }
    }

    async fn process_collect_tx(&self) {
        // 获取交易这里有问题
        let res = ApiCollectRepo::page_api_collect_with_status(
            &self.pool,
            0,
            1000,
            &[ApiCollectStatus::Init],
        )
        .await;
        match res {
            Ok((_, collect_txs)) => {
                for req in collect_txs {
                    self.process_collect_single_tx(req).await;
                }
            }
            Err(err) => {
                tracing::warn!(collect_tx=%err, "failed to collect tx");
            }
        }
    }

    async fn process_collect_single_tx(&self, req: ApiCollectEntity) {
        tracing::info!(trade_no=%req.trade_no, "process collect tx -------------------------------");
        let check_res = self.check_fee(&req).await;
        match check_res {
            Ok(pass) => {
                if !pass {
                    return;
                }
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "failed to process collect tx: {}", err);
                return self.handle_collect_tx_failed(&req.trade_no, err).await;
            }
        }

        if !self.check_digest(&req).await {
            tracing::error!(trade_no=%req.trade_no, "failed to validate failed");
            return self
                .handle_collect_tx_failed(
                    &req.trade_no,
                    ServiceError::Parameter("validate failed".to_string()),
                )
                .await;
        }

        let transfer_req_res = self.gen_transfer_req(&req).await;
        match transfer_req_res {
            Ok(transfer_req) => {
                // 发交易
                let tx_resp = ApiTransDomain::transfer(transfer_req).await;
                match tx_resp {
                    Ok(tx) => self.handle_collect_tx_success(&req.trade_no, tx).await,
                    Err(err) => {
                        tracing::error!(trade_no=%req.trade_no, "failed to process collect tx: {}", err);
                        self.handle_collect_tx_failed(&req.trade_no, err).await
                    }
                }
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "failed to process collect tx: {}", err);
            }
        }
    }

    async fn check_digest(&self, req: &ApiCollectEntity) -> bool {
        // check digest
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let mut d = Decimal::from_str(req.value.as_str()).unwrap();
        d = d.normalize();
        let raw_data = req.from_addr.clone() + req.to_addr.as_str() + d.to_string().as_str() + sn;
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(&raw_data));
        req.validate == digest
    }

    async fn gen_transfer_req(
        &self,
        req: &ApiCollectEntity,
    ) -> Result<ApiTransferReq, ServiceError> {
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

        Ok(ApiTransferReq { base: params, password: passwd })
    }

    async fn handle_collect_tx_success(&self, trade_no: &str, tx: TransferResp) {
        let resource_consume = if let Some(consumer) = tx.consumer {
            consumer.energy_used.to_string()
        } else {
            "0".to_string()
        };
        // 更新发送交易状态
        let res = ApiCollectRepo::update_api_collect_tx_status(
            &self.pool,
            trade_no,
            &tx.tx_hash,
            &resource_consume,
            &tx.fee,
            ApiCollectStatus::SendingTx,
        )
        .await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%trade_no, "send collect success ---");
                // 上报交易不影响交易偏移量计算
                let _ =
                    self.report_tx.send(ProcessCollectTxReportCommand::Tx(trade_no.to_string()));
            }
            Err(err) => {
                tracing::error!(trade_no=%trade_no, "update_api_collect_tx_status failed: {}", err);
            }
        }
    }

    async fn handle_collect_tx_failed(&self, trade_no: &str, err: ServiceError) {
        // 更新失败状态
        let res = ApiCollectRepo::update_api_collect_status(
            &self.pool,
            trade_no,
            ApiCollectStatus::SendingTxFailed,
            &err.to_string(),
        )
        .await;
        match res {
            Ok(_) => {
                // 上报交易不影响交易偏移量计算
                let _ =
                    self.report_tx.send(ProcessCollectTxReportCommand::Tx(trade_no.to_string()));
            }
            Err(err) => {
                tracing::error!(trade_no=%trade_no, "update_api_collect_status failed: {}", err);
            }
        }
    }
}

#[async_trait::async_trait]
trait CheckFee {
    async fn check_fee(&self, req: &ApiCollectEntity) -> Result<bool, ServiceError>;
    async fn query_balance(
        &self,
        owner_address: &str,
        chain_code: ChainCode,
        token_address: Option<String>,
        decimals: u8,
    ) -> Result<String, ServiceError>;
    async fn estimate_fee(
        &self,
        from: &str,
        to: &str,
        value: &str,
        chain_code: ChainCode,
        symbol: &str,
        main_symbol: &str,
        token_address: Option<String>,
        decimals: u8,
    ) -> Result<String, ServiceError>;

    async fn get_collect_config(
        &self,
        uid: &str,
        chain_code: &str,
    ) -> Result<ChainConfig, ServiceError>;
}

#[async_trait::async_trait]
impl CheckFee for ProcessCollectTx {
    async fn check_fee(&self, req: &ApiCollectEntity) -> Result<bool, ServiceError> {
        // 查询手续费
        let chain_code: ChainCode = req.chain_code.as_str().try_into()?;
        let main_coin = ChainTransDomain::main_coin(&req.chain_code).await?;
        tracing::info!(trade_no=%req.trade_no, "主币： {}", main_coin.symbol);
        let main_symbol = main_coin.symbol;
        let fee = self
            .estimate_fee(
                &req.from_addr,
                &req.to_addr,
                &req.value,
                chain_code,
                &req.symbol,
                &main_symbol,
                req.token_addr.clone(),
                main_coin.decimals,
            )
            .await?;
        tracing::info!(trade_no=%req.trade_no, "估算手续费: {}", fee);

        // 查询策略
        let chain_config = self.get_collect_config(&req.uid, &req.chain_code).await?;

        // 查询资产主币余额

        let balance =
            self.query_balance(&req.from_addr, chain_code, None, main_coin.decimals).await?;

        tracing::info!(trade_no=%req.trade_no, "from: {}, to: {}", req.from_addr, req.to_addr);
        tracing::info!(trade_no=%req.trade_no, "资产主币余额: {balance}, 手续费: {fee}");

        let balance = conversion::decimal_from_str(&balance)?;
        let value = conversion::decimal_from_str(&req.value)?;
        let fee_decimal = conversion::decimal_from_str(&fee.to_string())?;

        let need = if req.token_addr.is_some() { fee_decimal } else { fee_decimal + value };
        tracing::info!(trade_no=%req.trade_no, "need: {need}");
        // 如果手续费不足，则从其他地址转入手续费费用
        if need > Decimal::from(0) && balance < need {
            tracing::info!(trade_no=%req.trade_no, "need collect fee");

            let token =
                if let Some(token) = req.token_addr.clone() { token } else { "".to_string() };
            let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            let upload_req = ServiceFeeUploadReq::new(
                &req.trade_no,
                &req.chain_code,
                &main_symbol,
                token.as_str(),
                &chain_config.normal_address.address,
                &req.from_addr,
                unit::string_to_f64(&fee)?,
            );
            backend_api.upload_service_fee_record(&upload_req).await?;

            ApiCollectRepo::update_api_collect_status(
                &self.pool,
                &req.trade_no,
                ApiCollectStatus::InsufficientBalance,
                "insufficient balance",
            )
            .await?;

            Ok(false)
        } else {
            Ok(true)
        }
    }

    async fn query_balance(
        &self,
        owner_address: &str,
        chain_code: ChainCode,
        token_address: Option<String>,
        decimals: u8,
    ) -> Result<String, ServiceError> {
        let adapter = ApiChainAdapterFactory::new_transaction_adapter(chain_code).await?;
        let balance = adapter.balance(&owner_address, token_address).await?;
        let ammount = unit::format_to_string(balance, decimals)?;
        Ok(ammount)
    }

    async fn estimate_fee(
        &self,
        from: &str,
        to: &str,
        value: &str,
        chain_code: ChainCode,
        symbol: &str,
        main_symbol: &str,
        token_address: Option<String>,
        decimals: u8,
    ) -> Result<String, ServiceError> {
        let adapter = ApiChainAdapterFactory::new_transaction_adapter(chain_code).await?;
        let mut params = ApiBaseTransferReq::new(from, to, value, &chain_code.to_string());
        params.with_token(token_address, decimals, symbol);
        let fee = adapter.estimate_fee(params, main_symbol).await?;

        let amount = match chain_code {
            ChainCode::Tron => fee,
            ChainCode::Bitcoin => todo!(),
            ChainCode::Solana => fee,
            ChainCode::Ethereum => fee,
            ChainCode::BnbSmartChain => fee,
            ChainCode::Litecoin => todo!(),
            ChainCode::Dogcoin => todo!(),
            ChainCode::Sui => todo!(),
            ChainCode::Ton => todo!(),
        };
        // let amount = unit::convert_to_u256(&amount, decimals)?;
        Ok(amount)
    }

    async fn get_collect_config(
        &self,
        uid: &str,
        chain_code: &str,
    ) -> Result<ChainConfig, ServiceError> {
        // 查询策略
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let strategy = backend_api.query_collect_strategy(uid).await?;
        let Some(chain_config) =
            strategy.chain_configs.into_iter().find(|config| config.chain_code == chain_code)
        else {
            return Err(crate::error::business::BusinessError::ApiWallet(
                ApiWalletError::ChainConfigNotFound(chain_code.to_owned()),
            )
            .into());
        };
        Ok(chain_config)
    }
}
