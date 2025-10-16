use std::str::FromStr;
use crate::{
    domain::{
        api_wallet::{
            adapter_factory::ApiChainAdapterFactory, trans::ApiTransDomain, wallet::ApiWalletDomain,
        },
        chain::{TransferResp, transaction::ChainTransDomain},
        coin::CoinDomain,
    },
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use chrono::TimeDelta;
use rust_decimal::Decimal;
use tokio::{
    sync::{Mutex, broadcast, mpsc},
    task::JoinHandle,
};
use tokio::time::sleep;
use wallet_database::{
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::collect::ApiCollectRepo,
};
use wallet_transport_backend::request::api_wallet::{
    strategy::ChainConfig,
    transaction::{
        ServiceFeeUploadReq, TransAckType, TransEventAckReq, TransStatus, TransType,
        TxExecReceiptUploadReq,
    },
};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::{conversion, unit};
use wallet_ecdh::GLOBAL_KEY;

#[derive(Clone)]
pub(crate) enum ProcessCollectTxCommand {
    Tx(String),
}

#[derive(Clone)]
pub(crate) enum ProcessCollectTxReportCommand {
    Tx(String),
}

#[derive(Clone)]
pub(crate) enum ProcessCollectTxConfirmReportCommand {
    Tx(String),
}

#[derive(Debug)]
pub(crate) struct ProcessCollectTxHandle {
    shutdown_tx: broadcast::Sender<()>,
    tx_tx: mpsc::Sender<ProcessCollectTxCommand>,
    confirm_report_tx: mpsc::Sender<ProcessCollectTxConfirmReportCommand>,
    tx_handle: Mutex<Option<JoinHandle<Result<(), ServiceError>>>>,
    tx_report_handle: Mutex<Option<JoinHandle<Result<(), crate::error::service::ServiceError>>>>,
    tx_confirm_report_handle:
        Mutex<Option<JoinHandle<Result<(), crate::error::service::ServiceError>>>>,
}

impl ProcessCollectTxHandle {
    pub(crate) async fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let shutdown_rx1 = shutdown_tx.subscribe();
        let shutdown_rx2 = shutdown_tx.subscribe();
        let shutdown_rx3 = shutdown_tx.subscribe();
        let (tx_tx, tx_rx) = mpsc::channel(1);
        let (report_tx, report_rx) = mpsc::channel(1);
        // 发交易
        let mut tx = ProcessCollectTx::new(shutdown_rx1, tx_rx, report_tx);
        let tx_handle = tokio::spawn(async move { tx.run().await });
        // 上报交易
        let mut tx_report = ProcessCollectTxReport::new(shutdown_rx2, report_rx);
        let tx_report_handle = tokio::spawn(async move { tx_report.run().await });
        // 上报已经确认交易
        let (confirm_report_tx, confirm_report_rx) = mpsc::channel(1);
        let mut tx_confirm_report = ProcessFeeTxConfirmReport::new(shutdown_rx3, confirm_report_rx);
        let tx_confirm_report_handle = tokio::spawn(async move { tx_confirm_report.run().await });
        Self {
            shutdown_tx,
            tx_tx,
            confirm_report_tx,
            tx_handle: Mutex::new(Some(tx_handle)),
            tx_report_handle: Mutex::new(Some(tx_report_handle)),
            tx_confirm_report_handle: Mutex::new(Some(tx_confirm_report_handle)),
        }
    }

    pub(crate) async fn submit_tx(&self, trade_no: &str) -> Result<(), ServiceError> {
        let _ = self.tx_tx.send(ProcessCollectTxCommand::Tx(trade_no.to_string()));
        Ok(())
    }

    pub(crate) async fn submit_confirm_report_tx(
        &self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        let _ = self
            .confirm_report_tx
            .send(ProcessCollectTxConfirmReportCommand::Tx(trade_no.to_string()));
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

struct ProcessCollectTx {
    shutdown_rx: broadcast::Receiver<()>,
    tx_rx: mpsc::Receiver<ProcessCollectTxCommand>,
    report_tx: mpsc::Sender<ProcessCollectTxReportCommand>,
}

impl ProcessCollectTx {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        tx_rx: mpsc::Receiver<ProcessCollectTxCommand>,
        report_tx: mpsc::Sender<ProcessCollectTxReportCommand>,
    ) -> Self {
        Self { shutdown_rx, tx_rx, report_tx }
    }

    async fn run(&mut self) -> Result<(), ServiceError> {
        tracing::info!("starting process collect -------------------------------");
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
                                match self.process_collect_single_tx_by_trade_no(&trade_no).await {
                                    Ok(_) => {}
                                    Err(err) => {
                                        tracing::error!("failed to process collect tx: {}", err);
                                    }
                                }
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    match self.process_collect_tx().await {
                        Ok(_) => {}
                        Err(err) => {
                            tracing::error!("failed to process collect tx: {}", err);
                        }
                    }
                }
            }
        }
        tracing::info!("closing process collect tx ------------------------------- end");
        Ok(())
    }

    async fn process_collect_single_tx_by_trade_no(
        &self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiCollectRepo::get_api_collect_by_trade_no_status(
            &pool,
            &trade_no,
            &[ApiCollectStatus::Init],
        )
        .await;
        if res.is_ok() {
            self.process_collect_single_tx(res.unwrap()).await
        } else {
            Err(ServiceError::Business(ApiWalletError::OrderNotFound(trade_no.to_string()).into()))
        }
    }

    async fn process_collect_tx(&self) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // 获取交易这里有问题
        let (_, transfer_fees) = ApiCollectRepo::page_api_collect_with_status(
            &pool.clone(),
            0,
            1000,
            &[ApiCollectStatus::Init],
        )
        .await?;
        for req in transfer_fees {
            self.process_collect_single_tx(req).await?;
        }
        Ok(())
    }

    async fn check_fee(&self, req: &ApiCollectEntity) -> Result<bool, ServiceError> {
        // 查询手续费
        let chain_code: ChainCode = req.chain_code.as_str().try_into()?;
        let main_coin = ChainTransDomain::main_coin(&req.chain_code).await?;
        tracing::info!("主币： {}", main_coin.symbol);
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
        tracing::info!("估算手续费: {}", fee);

        // 查询策略
        let chain_config = self.get_collect_config(&req.uid, &req.chain_code).await?;

        // 查询资产主币余额

        let balance =
            self.query_balance(&req.from_addr, chain_code, None, main_coin.decimals).await?;

        tracing::info!("from: {}, to: {}", req.from_addr, req.to_addr);
        tracing::info!("资产主币余额: {balance}, 手续费: {fee}");

        let balance = conversion::decimal_from_str(&balance)?;
        let value = conversion::decimal_from_str(&req.value)?;
        let fee_decimal = conversion::decimal_from_str(&fee.to_string())?;
        let need = if req.token_addr.is_some() { fee_decimal } else { fee_decimal + value };
        tracing::info!("need: {need}");
        // 如果手续费不足，则从其他地址转入手续费费用
        if balance < need {
            tracing::info!("need collect fee");
            let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
            ApiCollectRepo::update_api_collect_status(
                &pool,
                &req.trade_no,
                ApiCollectStatus::InsufficientBalance,
                "insufficient balance",
            )
            .await?;

            let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            let req = ServiceFeeUploadReq::new(
                &req.trade_no,
                &req.chain_code,
                &main_symbol,
                "",
                &chain_config.normal_address.address,
                &req.from_addr,
                unit::string_to_f64(&fee)?,
            );

            backend_api.upload_service_fee_record(&req).await?;
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
        let account = adapter.balance(&owner_address, token_address).await?;
        let ammount = unit::format_to_string(account, decimals)?;
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
            ChainCode::Solana => todo!(),
            ChainCode::Ethereum => todo!(),
            ChainCode::BnbSmartChain => todo!(),
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

    async fn process_collect_single_tx(&self, req: ApiCollectEntity) -> Result<(), ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "process collect tx -------------------------------");
        let pass = self.check_fee(&req).await?;
        if !pass {
            return Ok(());
        }
        // check
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let mut d = Decimal::from_str(req.value.as_str()).unwrap();
        d = d.normalize();
        let raw_data = req.from_addr.clone() + req.to_addr.as_str() + d.to_string().as_str() + sn;
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(&raw_data));
        if req.validate != digest {
            return self.handle_collect_tx_failed(&req.trade_no, ServiceError::Parameter("validate failed".to_string())).await
        }

        let coin =
            CoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_addr.clone()).await?;
        let mut params = ApiBaseTransferReq::new(
            &req.from_addr,
            &req.to_addr,
            &req.value,
            &req.chain_code,
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
            Ok(tx) => self.handle_collect_tx_success(&req.trade_no, tx).await,
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "failed to process collect tx: {}", err);
                self.handle_collect_tx_failed(&req.trade_no, err).await
            }
        }
    }

    async fn handle_collect_tx_success(
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
        ApiCollectRepo::update_api_collect_tx_status(
            &pool,
            trade_no,
            &tx.tx_hash,
            &resource_consume,
            &tx.fee,
            ApiCollectStatus::SendingTx,
        )
        .await?;
        tracing::info!("send collect success ---");
        // 上报交易不影响交易偏移量计算
        let _ = self.report_tx.send(ProcessCollectTxReportCommand::Tx(trade_no.to_string()));
        Ok(())
    }

    async fn handle_collect_tx_failed(
        &self,
        trade_no: &str,
        err: ServiceError,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiCollectRepo::update_api_collect_status(
            &pool,
            trade_no,
            ApiCollectStatus::SendingTxFailed,
            &err.to_string(),
        )
        .await?;
        // 上报交易不影响交易偏移量计算
        let _ = self.report_tx.send(ProcessCollectTxReportCommand::Tx(trade_no.to_string()));
        Ok(())
    }
}

struct ProcessCollectTxReport {
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessCollectTxReportCommand>,
    failed_count: i64,
}

impl ProcessCollectTxReport {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessCollectTxReportCommand>,
    ) -> Self {
        Self { shutdown_rx, report_rx, failed_count: 0 }
    }

    async fn run(&mut self) -> Result<(), ServiceError> {
        tracing::info!("starting process collect tx report -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process collect tx report -------------------------------");
                    break;
                }
                report_msg = self.report_rx.recv() => {
                    if let Some(cmd) = report_msg {
                        match cmd {
                            ProcessCollectTxReportCommand::Tx(trade_no) => {
                                match self.process_collect_single_tx_report_by_trade_no(&trade_no).await {
                                    Ok(_) => {}
                                    Err(err) => {
                                        tracing::error!("failed to process collect tx report: {}", err);
                                    }
                                }
                            }
                        }
                        iv.reset();
                    }
                }
                _ = iv.tick() => {
                    match self.process_collect_tx_report().await {
                        Ok(_) => {}
                        Err(err) => {
                            tracing::error!("failed to process collect tx report: {}", err);
                        }
                    }
                }
            }
        }
        tracing::info!("closing process collect tx report ------------------------------- end");
        Ok(())
    }

    async fn process_collect_single_tx_report_by_trade_no(
        &self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiCollectRepo::get_api_collect_by_trade_no_status(
            &pool,
            &trade_no,
            &[ApiCollectStatus::SendingTx, ApiCollectStatus::SendingTxFailed],
        )
        .await;
        if res.is_ok() {
            self.process_collect_single_tx_report(res.unwrap()).await?;
            Ok(())
        } else {
            Err(ServiceError::Business(ApiWalletError::OrderNotFound(trade_no.to_string()).into()))
        }
    }

    async fn process_collect_tx_report(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let (_, transfer_fees) = ApiCollectRepo::page_api_collect_with_status(
            &pool,
            0,
            1000 + self.failed_count,
            &[ApiCollectStatus::SendingTx, ApiCollectStatus::SendingTxFailed],
        )
        .await?;
        let transfer_fees_len = transfer_fees.len();
        let mut failed_count = 0;
        for req in transfer_fees {
            if let Err(_) = self.process_collect_single_tx_report(req).await {
                failed_count += 1;
            }
        }
        if failed_count == transfer_fees_len as i32 {
            self.failed_count += 1;
        }
        Ok(())
    }

    async fn process_collect_single_tx_report(
        &self,
        req: ApiCollectEntity,
    ) -> Result<i32, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "process collect tx report -------------------------------");
        // 判断超时时间
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(1 << req.post_tx_count as i64) {
            tracing::warn!(trade_no=%req.trade_no, "process collect tx report timeout ---");
            return Ok(0);
        }
        let status = if req.status == ApiCollectStatus::SendingTxFailed {
            TransStatus::Fail
        } else {
            TransStatus::Success
        };
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend_api
            .upload_tx_exec_receipt(&TxExecReceiptUploadReq::new(
                &req.trade_no,
                TransType::Col,
                &req.tx_hash,
                status,
                &req.notes,
            ))
            .await
        {
            Ok(_) => {
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                if req.status == ApiCollectStatus::SendingTxFailed {
                    ApiCollectRepo::update_api_collect_next_status(
                        &pool,
                        &req.trade_no,
                        ApiCollectStatus::SendingTxFailed,
                        ApiCollectStatus::SendingTxFailedReport,
                        "uploaded tx ok",
                    )
                    .await?;
                } else {
                    ApiCollectRepo::update_api_collect_next_status(
                        &pool,
                        &req.trade_no,
                        ApiCollectStatus::SendingTx,
                        ApiCollectStatus::SendingTxReport,
                        "uploaded tx ok",
                    )
                    .await?;
                }
                tracing::info!("upload tx exec receipt success ---");
                Ok(1)
            }
            Err(err) => {
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                if req.status == ApiCollectStatus::SendingTx {
                    ApiCollectRepo::update_api_collect_post_tx_count(
                        &pool,
                        &req.trade_no,
                        ApiCollectStatus::SendingTx,
                    )
                    .await?;
                } else {
                    ApiCollectRepo::update_api_collect_post_tx_count(
                        &pool,
                        &req.trade_no,
                        ApiCollectStatus::SendingTxFailed,
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
    report_rx: mpsc::Receiver<ProcessCollectTxConfirmReportCommand>,
    failed_count: i64,
}

impl ProcessFeeTxConfirmReport {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessCollectTxConfirmReportCommand>,
    ) -> Self {
        Self { shutdown_rx, report_rx, failed_count: 0 }
    }

    async fn run(&mut self) -> Result<(), ServiceError> {
        tracing::info!(
            "starting process collect tx confirm report -------------------------------"
        );
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process collect tx confirm report -------------------------------");
                    break;
                }
                report_msg = self.report_rx.recv() => {
                    match report_msg {
                        Some(cmd) => {
                            match cmd {
                                ProcessCollectTxConfirmReportCommand::Tx(trade_no) => {
                                    match self.process_fee_single_tx_confirm_report_by_trade_no(&trade_no).await {
                                        Ok(_) => {}
                                        Err(err) => {
                                            tracing::error!("failed to process collect tx confirm report: {}", err);
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
                    match self.process_collect_tx_confirm_report().await {
                        Ok(_) => {}
                        Err(err) => {
                            tracing::error!("failed to process collect tx confirm report: {}", err);
                        }
                    }
                }
            }
        }
        tracing::info!(
            "closing process collect tx confirm report ------------------------------- end"
        );
        Ok(())
    }

    async fn process_fee_single_tx_confirm_report_by_trade_no(
        &self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiCollectRepo::get_api_collect_by_trade_no_status(
            &pool,
            &trade_no,
            &[ApiCollectStatus::Failure, ApiCollectStatus::Success],
        )
        .await;
        if res.is_ok() {
            self.process_collect_single_tx_confirm_report(res.unwrap()).await?;
            Ok(())
        } else {
            Err(ServiceError::Business(ApiWalletError::OrderNotFound(trade_no.to_string()).into()))
        }
    }

    async fn process_collect_tx_confirm_report(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let (_, transfer_fees) = ApiCollectRepo::page_api_collect_with_status(
            &pool,
            0,
            1000 + self.failed_count,
            &[ApiCollectStatus::Failure, ApiCollectStatus::Success],
        )
        .await?;
        let transfer_fees_len = transfer_fees.len();
        let mut failed_count = 0;
        for req in transfer_fees {
            if let Err(_) = self.process_collect_single_tx_confirm_report(req).await {
                failed_count += 1;
            }
        }
        if failed_count == transfer_fees_len as i32 {
            self.failed_count += 1;
        }
        Ok(())
    }

    async fn process_collect_single_tx_confirm_report(
        &self,
        req: ApiCollectEntity,
    ) -> Result<(), ServiceError> {
        tracing::info!(id=%req.id,hash=%req.tx_hash,status=%req.status, "process_collect_single_tx_confirm_report ---------------------------------4");
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(req.post_confirm_tx_count as i64) {
            tracing::warn!(
                "process_withdraw_single_tx_confirm_report timeout post confirm_tx_count is too long"
            );
            return Ok(());
        }
        if req.status == ApiCollectStatus::SendingTxFailed {
            tracing::warn!("process_withdraw_single_tx_confirm_report status is wrong");
            return Ok(());
        };
        if !(req.status == ApiCollectStatus::Success || req.status == ApiCollectStatus::Failure) {
            tracing::warn!(
                "process_collect_single_tx_confirm_report status is wrong {}",
                req.status
            );
            return Ok(());
        }
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend_api
            .trans_event_ack(&TransEventAckReq::new(
                &req.trade_no,
                TransType::Col,
                TransAckType::TxRes,
            ))
            .await
        {
            Ok(_) => {
                tracing::info!("process_collect_single_tx_confirm_report success");
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                ApiCollectRepo::update_api_collect_next_status(
                    &pool,
                    &req.trade_no,
                    req.status,
                    ApiCollectStatus::ReceivedConfirmReport,
                    "trans event ack"
                )
                .await?;
                return Ok(());
            }
            Err(err) => {
                tracing::error!("failed to process withdraw tx confirm report: {}", err);
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                ApiCollectRepo::update_api_collect_post_confirm_tx_count(
                    &pool,
                    &req.trade_no,
                    req.status,
                )
                .await?;
                return Err(ServiceError::TransportBackend(err.into()));
            }
        }
        Ok(())
    }
}
