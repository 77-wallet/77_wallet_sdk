use crate::{
    domain::{
        api_wallet::{
            adapter_factory::ApiChainAdapterFactory, chain::ApiChainTransDomain,
            coin::ApiCoinDomain, trans::ApiTransDomain, wallet::ApiWalletDomain,
        },
        chain::TransferResp,
    },
    error::{business::api_wallet::ApiWalletError, service::ServiceError, system::SystemError},
    infrastructure::collect::command::{ProcessCollectTxCommand, ProcessCollectTxReportCommand},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
    response_vo::{CommonFeeDetails, EthereumFeeDetails, FeeDetailsVo, TronFeeDetails},
};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use std::{str::FromStr, sync::Arc};
use tokio::{
    sync::{broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::{collect::ApiCollectRepo, nonce::ApiNonceRepo},
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
        tracing::info!("process_collect_tx_send: 启动归集交易处理循环");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                tracing::warn!("process_collect_tx_send: 共享密钥未设置，等待10秒后重试");
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("process_collect_tx_send: 接收到关闭信号，退出处理循环");
                    break;
                }
                msg = self.tx_rx.recv() => {
                    if let Some(cmd) = msg {
                        match cmd {
                            ProcessCollectTxCommand::Tx(trade_no) => {
                                tracing::info!(trade_no=%trade_no, "process_collect_tx_send: 接收到单个交易处理请求");
                                if let Err(err) = self.process_collect_single_tx_by_trade_no(&trade_no).await {
                                    tracing::error!(trade_no=%trade_no, "process_collect_tx_send: 处理单个交易失败: {}", err);
                                }
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    tracing::info!("process_collect_tx_send: 执行定时批量处理归集交易");
                    if let Err(err) = self.process_collect_tx().await {
                        tracing::error!("process_collect_tx_send: 处理批量归集交易失败: {}", err);
                    }
                }
            }
        }
    }

    async fn process_collect_single_tx_by_trade_no(
        &self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        let res = ApiCollectRepo::get_api_collect_by_trade_no_status(
            &self.pool,
            &trade_no,
            &[ApiCollectStatus::Init],
        )
        .await;
        match res {
            Ok(res) => self.process_collect_single_tx(res).await,
            Err(err) => {
                tracing::warn!(trade_no=%trade_no, "process collect tx not found: {}", err);
                Err(err.into())
            }
        }
    }

    async fn process_collect_tx(&self) -> Result<(), ServiceError> {
        tracing::info!("process_collect_tx_send: 查询待处理的归集交易");
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
                tracing::info!(
                    "process_collect_tx_send: 找到 {} 笔待处理的归集交易",
                    collect_txs.len()
                );
                for req in collect_txs {
                    let trade_no = req.trade_no.clone(); // 提前克隆trade_no
                    if let Err(err) = self.process_collect_single_tx(req).await {
                        tracing::error!(trade_no=%trade_no, "process_collect_tx_send: 处理单个归集交易失败: {}", err);
                    }
                }
                Ok(())
            }
            Err(err) => {
                tracing::warn!(error=%err, "process_collect_tx_send: 查询待处理归集交易失败");
                Err(err.into())
            }
        }
    }

    async fn process_collect_single_tx(&self, req: ApiCollectEntity) -> Result<(), ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 开始处理归集交易, from={}, to={}, value={}, chain={}, symbol={}", 
            req.from_addr, req.to_addr, req.value, req.chain_code, req.symbol);

        // 检查手续费
        let check_res = self.check_fee(&req).await;
        let trade_no = &req.trade_no;
        match check_res {
            Ok(pass) => {
                if !pass {
                    tracing::info!(trade_no=%trade_no, "process_collect_tx_send: 手续费不足，已请求补充");
                    return Ok(());
                }
                tracing::info!(trade_no=%trade_no, "process_collect_tx_send: 手续费检查通过");
            }
            Err(err) => {
                tracing::error!(trade_no=%trade_no, "process_collect_tx_send: 手续费检查失败: {}", err);
                return self.handle_collect_tx_failed(&trade_no, err).await;
            }
        }

        // 检查交易摘要
        if !self.check_digest(&req).await {
            tracing::error!(trade_no=%trade_no, "process_collect_tx_send: 交易摘要验证失败");
            return self
                .handle_collect_tx_failed(
                    &trade_no,
                    ServiceError::Parameter("交易摘要验证失败".to_string()),
                )
                .await;
        }
        tracing::info!(trade_no=%trade_no, "process_collect_tx_send: 交易摘要验证通过");

        // 生成转账请求
        let transfer_req_res = self.gen_transfer_req(&req).await;
        match transfer_req_res {
            Ok(transfer_req) => {
                tracing::info!(trade_no=%trade_no, "process_collect_tx_send: 生成转账请求成功，准备发送交易");

                // 发送交易
                let nonce = transfer_req.nonce;
                tracing::info!(trade_no=%trade_no, "process_collect_tx_send: 开始发送归集交易, nonce={}", nonce);

                let tx_resp = ApiTransDomain::transfer(transfer_req).await;
                match tx_resp {
                    Ok(tx) => {
                        tracing::info!(trade_no=%trade_no, "process_collect_tx_send: 发送交易成功, tx_hash={}", tx.tx_hash);
                        return self.handle_collect_tx_success(req, tx, nonce).await;
                    }
                    Err(err) => {
                        tracing::error!(trade_no=%trade_no, "process_collect_tx_send: 发送交易失败: {}", err);
                        return self.handle_collect_tx_failed(&trade_no, err).await;
                    }
                }
            }
            Err(err) => {
                tracing::error!(trade_no=%trade_no, "process_collect_tx_send: 生成转账请求失败: {}", err);
                return self.handle_collect_tx_failed(&trade_no, err).await;
            }
        }
    }

    async fn check_digest(&self, req: &ApiCollectEntity) -> bool {
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 开始验证交易摘要");
        // check digest
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let mut d = Decimal::from_str(req.value.as_str()).unwrap();
        d = d.normalize();
        let raw_data = req.from_addr.clone() + req.to_addr.as_str() + d.to_string().as_str() + sn;
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(&raw_data));

        let is_valid = req.validate == digest;
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 交易摘要验证完成, 结果: {}", is_valid);
        is_valid
    }

    async fn get_eth_nonce(&self, from_addr: &str, chain_code: &str) -> Result<i64, ServiceError> {
        tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "process_collect_tx_send: 获取以太坊nonce");
        match ApiNonceRepo::get_api_nonce(&self.pool, from_addr, chain_code).await {
            Ok(nonce) => {
                let next_nonce = nonce + 1;
                tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "process_collect_tx_send: 从本地缓存获取nonce: {}, 下一个nonce: {}", nonce, next_nonce);
                Ok(next_nonce)
            }
            Err(_) => {
                tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "process_collect_tx_send: 本地缓存未找到nonce，从链上获取");
                let nonce = ApiTransDomain::nonce(from_addr, chain_code).await?;
                tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "process_collect_tx_send: 从链上获取nonce: {}", nonce);
                Ok(nonce as i64)
            }
        }
    }

    async fn gen_transfer_req(
        &self,
        req: &ApiCollectEntity,
    ) -> Result<ApiTransferReq, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 开始生成转账请求");

        // 获取币种信息
        let coin =
            ApiCoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_addr.clone()).await?;
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 获取币种信息成功, symbol={}, token_address={:?}, decimals={}", 
            coin.symbol, coin.token_address, coin.decimals);

        // 创建基础转账请求
        let mut params =
            ApiBaseTransferReq::new(&req.from_addr, &req.to_addr, &req.value, &req.chain_code);
        let token_address = if coin.token_address.is_none() {
            None
        } else {
            let s = coin.token_address.unwrap();
            if s.is_empty() { None } else { Some(s) }
        };
        params.with_token(token_address, coin.decimals, &coin.symbol);
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 创建基础转账请求成功");

        // 获取钱包密码
        let passwd = ApiWalletDomain::get_passwd().await?;
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 获取钱包密码成功");

        // 计算nonce
        let chain_code = req.chain_code.as_str();
        let chain_code: ChainCode = chain_code.try_into()?;
        let nonce: i64 = match chain_code {
            ChainCode::Tron => 0,
            ChainCode::Bitcoin => 0,
            ChainCode::Solana => 0,
            ChainCode::Ethereum => self.get_eth_nonce(&req.from_addr, &req.chain_code).await?,
            ChainCode::BnbSmartChain => self.get_eth_nonce(&req.from_addr, &req.chain_code).await?,
            ChainCode::Litecoin => 0,
            ChainCode::Dogcoin => 0,
            ChainCode::Sui => 0,
            ChainCode::Ton => 0,
        };
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 计算nonce成功, nonce={}", nonce);

        let transfer_req = ApiTransferReq { base: params, password: passwd, nonce: nonce as u64 };
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 生成转账请求成功");
        Ok(transfer_req)
    }

    async fn handle_collect_tx_success(
        &self,
        req: ApiCollectEntity,
        tx: TransferResp,
        nonce: u64,
    ) -> Result<(), ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 处理交易成功结果");

        let resource_consume = if let Some(consumer) = tx.consumer {
            consumer.energy_used.to_string()
        } else {
            "0".to_string()
        };

        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 交易资源消耗: {}, 手续费: {}", resource_consume, tx.fee);

        // 更新交易状态
        let res = if req.chain_code == ChainCode::Ethereum.to_string()
            || req.chain_code == ChainCode::BnbSmartChain.to_string()
        {
            tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 更新以太坊/BSC交易状态，包含nonce");
            ApiCollectRepo::update_api_collect_tx_status_nonce(
                &self.pool,
                &req.from_addr,
                &req.chain_code,
                &req.trade_no,
                nonce as i64,
                &tx.tx_hash,
                &resource_consume,
                &tx.fee,
                ApiCollectStatus::SendingTx,
            )
            .await
        } else {
            // 更新发送交易状态
            tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 更新非以太坊/BSC交易状态");
            ApiCollectRepo::update_api_collect_tx_status(
                &self.pool,
                &req.trade_no,
                &tx.tx_hash,
                &resource_consume,
                &tx.fee,
                ApiCollectStatus::SendingTx,
            )
            .await
        };

        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 更新交易状态成功，交易已发送");
                // 上报交易不影响交易偏移量计算
                tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 准备上报交易结果");
                self.report_tx
                    .send(ProcessCollectTxReportCommand::Tx(req.trade_no.to_string()))
                    .await
                    .map_err(|e| {
                        ServiceError::System(SystemError::ChannelSendFailed(e.to_string()))
                    })?;
                tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 交易上报完成");
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "process_collect_tx_send: 更新交易状态失败: {}", err);
            }
        }
        Ok(())
    }

    async fn handle_collect_tx_failed(
        &self,
        trade_no: &str,
        err: ServiceError,
    ) -> Result<(), ServiceError> {
        tracing::info!(trade_no=%trade_no, "process_collect_tx_send: 处理交易失败结果, 错误: {}", err);

        // 更新失败状态
        let res = ApiCollectRepo::update_api_collect_status_and_err(
            &self.pool,
            trade_no,
            ApiCollectStatus::SendingTxFailed,
            101,
            &err.to_string(),
        )
        .await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%trade_no, "process_collect_tx_send: 更新交易状态为失败成功");
                // 上报交易不影响交易偏移量计算
                tracing::info!(trade_no=%trade_no, "process_collect_tx_send: 准备上报失败交易");
                self.report_tx
                    .send(ProcessCollectTxReportCommand::Tx(trade_no.to_string()))
                    .await
                    .map_err(|e| {
                        ServiceError::System(SystemError::ChannelSendFailed(e.to_string()))
                    })?;
                tracing::info!(trade_no=%trade_no, "process_collect_tx_send: 失败交易上报完成");
            }
            Err(err) => {
                tracing::error!(trade_no=%trade_no, "process_collect_tx_send: 更新失败状态失败: {}", err);
            }
        }
        Ok(())
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
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 开始检查手续费, from={}, to={}, value={}, token={:?}", 
            req.from_addr, req.to_addr, req.value, req.token_addr);

        // 查询主币信息
        let chain_code: ChainCode = req.chain_code.as_str().try_into()?;
        let main_coin = ApiChainTransDomain::main_coin(&req.chain_code).await?;
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 主币信息: symbol={}, decimals={}", main_coin.symbol, main_coin.decimals);

        // 确定代币信息
        let (token_symbol, token, token_decimals) = if let Some(token) = req.token_addr.clone() {
            if token.is_empty() {
                (main_coin.symbol.clone(), None, main_coin.decimals)
            } else {
                let token_coin =
                    ApiCoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_addr.clone())
                        .await?;
                tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 代币信息: symbol={}, token_address={:?}, decimals={}", 
                    token_coin.symbol, token_coin.token_address, token_coin.decimals);
                (token_coin.symbol, token_coin.token_address, token_coin.decimals)
            }
        } else {
            (main_coin.symbol.clone(), None, main_coin.decimals)
        };

        // 估算手续费
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 开始估算手续费");
        let fee_str = self
            .estimate_fee(
                &req.from_addr,
                &req.to_addr,
                &req.value,
                chain_code,
                &token_symbol,
                &main_coin.symbol,
                token,
                token_decimals,
            )
            .await?;
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 估算手续费完成: {}", fee_str);

        // 查询资产主币余额
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 查询主币余额");
        let balance =
            self.query_balance(&req.from_addr, chain_code, None, main_coin.decimals).await?;
        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 主币余额查询完成: {}", balance);

        // 计算所需金额
        let balance = conversion::decimal_from_str(&balance)?;
        let mut fee = conversion::decimal_from_str(&fee_str)?;

        // Solana特殊处理
        if chain_code == ChainCode::Solana {
            if balance <= Decimal::from(0) {
                fee = fee + Decimal::from_str("0.002").unwrap();
                tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: Solana余额为0，增加0.002额外手续费, 总手续费: {}", fee);
            }
        }

        // 计算需要的总金额
        let need = if req.token_addr.is_some() {
            // 代币交易只需要手续费
            tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 代币交易，只需要手续费");
            fee
        } else {
            // 主币交易需要手续费+转账金额
            let value = conversion::decimal_from_str(&req.value)?;
            tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 主币交易，需要手续费+转账金额, value={}", value);
            fee + value
        };

        tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 手续费检查结果 - 可用余额: {}, 需要金额: {}, 手续费: {}", balance, need, fee);

        // 如果手续费不足，则从其他地址转入手续费费用
        if fee > Decimal::from(0) && balance < need {
            tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 手续费不足，需要请求补充");

            // 查询策略
            tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 查询归集策略");
            let chain_config = self.get_collect_config(&req.uid, &req.chain_code).await?;
            tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 获取归集策略成功, 正常地址: {}", chain_config.normal_address.address);

            // 计算需要补充的手续费
            let mut fee_to_upload = if let Some(f) = fee.to_f64() { f } else { 0.0 };
            if chain_code == ChainCode::Ethereum || chain_code == ChainCode::BnbSmartChain {
                fee_to_upload = fee_to_upload * 2.0;
                tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 以太坊/BSC网络，手续费翻倍: {}", fee_to_upload);
            }

            // 上传手续费记录
            let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            let upload_req = ServiceFeeUploadReq::new(
                &req.trade_no,
                &req.chain_code,
                &main_coin.symbol,
                "",
                &chain_config.normal_address.address,
                &req.from_addr,
                fee_to_upload,
            );

            tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 上传手续费记录");
            backend_api.upload_service_fee_record(&upload_req).await?;
            tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 上传手续费记录成功");

            // 更新交易状态为余额不足
            tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 更新交易状态为余额不足");
            ApiCollectRepo::update_api_collect_status_and_err(
                &self.pool,
                &req.trade_no,
                ApiCollectStatus::InsufficientBalance,
                102,
                "insufficient balance",
            )
            .await?;
            tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 更新交易状态完成");

            Ok(false)
        } else {
            tracing::info!(trade_no=%req.trade_no, "process_collect_tx_send: 手续费充足，继续交易");
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
        tracing::info!(owner_address=%owner_address, chain_code=%chain_code.to_string(), token_address=%token_address.as_deref().unwrap_or(""), 
            "process_collect_tx_send: 查询余额");

        // Log token_address before moving it to adapter.balance
        let token_address_log = token_address.clone();
        let adapter = ApiChainAdapterFactory::new_transaction_adapter(chain_code).await?;
        let balance = adapter.balance(&owner_address, token_address).await?;
        let amount = unit::format_to_string(balance, decimals)?;

        tracing::info!(owner_address=%owner_address, chain_code=%chain_code.to_string(), token_address=%token_address_log.as_deref().unwrap_or(""), 
            "process_collect_tx_send: 查询余额完成: {}", amount);
        Ok(amount)
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
        // TODO: 可优化速度
        tracing::info!(from=%from, to=%to, value=%value, chain_code=%chain_code.to_string(), symbol=%symbol,
            main_symbol=%main_symbol, token_address=%token_address.as_deref().unwrap_or(""), 
            "process_collect_tx_send: 估算交易手续费");

        let adapter = ApiChainAdapterFactory::new_transaction_adapter(chain_code).await?;
        let mut params = ApiBaseTransferReq::new(from, to, value, &chain_code.to_string());
        params.with_token(token_address, decimals, symbol);

        let fee = adapter.estimate_fee(params, main_symbol).await?;

        let amount = match chain_code {
            ChainCode::Tron => {
                let res: TronFeeDetails = wallet_utils::serde_func::serde_from_str(&fee)?;
                res.estimate_fee.amount.to_string()
            }
            ChainCode::Bitcoin => todo!(),
            ChainCode::Solana => {
                let res: CommonFeeDetails = wallet_utils::serde_func::serde_from_str(&fee)?;
                res.estimate_fee.amount.to_string()
            }
            ChainCode::Ethereum => {
                let res: FeeDetailsVo<EthereumFeeDetails> =
                    wallet_utils::serde_func::serde_from_str(&fee)?;
                let mut amount: f64 = 0.0;
                for it in res.data {
                    amount = amount + it.estimate_fee.amount;
                }
                amount.to_string()
            }
            ChainCode::BnbSmartChain => {
                let res: FeeDetailsVo<EthereumFeeDetails> =
                    wallet_utils::serde_func::serde_from_str(&fee)?;
                let mut amount: f64 = 0.0;
                for it in res.data {
                    amount = amount + it.estimate_fee.amount;
                }
                amount.to_string()
            }
            ChainCode::Litecoin => todo!(),
            ChainCode::Dogcoin => todo!(),
            ChainCode::Sui => todo!(),
            ChainCode::Ton => todo!(),
        };

        tracing::info!(from=%from, to=%to, chain_code=%chain_code.to_string(), "process_collect_tx_send: 估算手续费完成: {}", amount);
        Ok(amount)
    }

    async fn get_collect_config(
        &self,
        uid: &str,
        chain_code: &str,
    ) -> Result<ChainConfig, ServiceError> {
        tracing::info!(uid=%uid, chain_code=%chain_code, "process_collect_tx_send: 查询归集策略");

        // 查询策略
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let strategy = backend_api.query_collect_strategy(uid).await?;

        tracing::info!(uid=%uid, "process_collect_tx_send: 获取归集策略成功，包含 {} 条链配置", strategy.chain_configs.len());

        let Some(chain_config) =
            strategy.chain_configs.into_iter().find(|config| config.chain_code == chain_code)
        else {
            tracing::error!(uid=%uid, chain_code=%chain_code, "process_collect_tx_send: 未找到对应的链配置");
            return Err(crate::error::business::BusinessError::ApiWallet(
                ApiWalletError::ChainConfigNotFound(chain_code.to_owned()),
            )
            .into());
        };

        tracing::info!(uid=%uid, chain_code=%chain_code, "process_collect_tx_send: 找到链配置, normal_address={}", chain_config.normal_address.address);
        Ok(chain_config)
    }
}
