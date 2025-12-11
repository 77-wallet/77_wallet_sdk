use crate::{
    context::Context,
    domain::{
        api_wallet::{
            account::ApiAccountDomain, coin::ApiCoinDomain, trans::ApiTransDomain,
            wallet::ApiWalletDomain,
        },
        chain::TransferResp,
    },
    error::{service::ServiceError, system::SystemError},
    infrastructure::collect_fee::command::{ProcessFeeTxCommand, ProcessFeeTxReportCommand},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use dashmap::DashMap;
use rust_decimal::Decimal;
use std::{str::FromStr, sync::Arc};
use tokio::{
    sync::{broadcast, mpsc},
    time::sleep,
};
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_database::{
    entities::api_fee::{ApiFeeEntity, ApiFeeStatus},
    repositories::api_wallet::{fee::ApiFeeRepo, nonce::ApiNonceRepo},
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_types::chain::chain::ChainCode;

pub(super) struct ProcessFeeTx {
    ctx: &'static Context,
    pool: Arc<sqlx::SqlitePool>,
    shutdown_rx: broadcast::Receiver<()>,
    tx_rx: mpsc::Receiver<ProcessFeeTxCommand>,
    report_tx: mpsc::Sender<ProcessFeeTxReportCommand>,
    private_key_cache: Arc<DashMap<(String, String), ChainPrivateKey>>,
}

impl ProcessFeeTx {
    pub(super) fn new(
        ctx: &'static Context,
        pool: Arc<sqlx::SqlitePool>,
        shutdown_rx: broadcast::Receiver<()>,
        tx_rx: mpsc::Receiver<ProcessFeeTxCommand>,
        report_tx: mpsc::Sender<ProcessFeeTxReportCommand>,
    ) -> Self {
        Self {
            ctx,
            pool,
            shutdown_rx,
            tx_rx,
            report_tx,
            private_key_cache: Arc::new(DashMap::new()),
        }
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
        tracing::info!(trade_no=%trade_no, "[手续费归集] 根据交易编号处理单个手续费交易");
        let res = ApiFeeRepo::get_api_fee_by_trade_no_status(
            &self.pool,
            &trade_no,
            &[ApiFeeStatus::Init],
        )
        .await;
        match res {
            Ok(fee) => {
                tracing::info!(trade_no=%trade_no, "[手续费归集] 找到待处理的手续费交易记录");
                if let Err(err) = self.process_fee_single_tx(fee).await {
                    tracing::error!(trade_no=%trade_no, "[手续费归集] 处理单个手续费交易失败: {:?}", err);
                }
            }
            Err(err) => {
                tracing::error!(trade_no=%trade_no, "[手续费归集] 获取手续费交易记录失败: {:?}", err);
            }
        }
    }

    async fn process_fee_tx(&self) {
        tracing::info!("[手续费归集] 批量处理手续费交易");
        // 获取交易这里有问题
        let res =
            ApiFeeRepo::page_api_fee_with_status(&self.pool, 0, 1000, &[ApiFeeStatus::Init]).await;
        match res {
            Ok((_, transfer_fees)) => {
                tracing::info!(
                    "[手续费归集] 找到 {} 条待处理的手续费交易记录",
                    transfer_fees.len()
                );
                for req in transfer_fees {
                    let trade_no = req.trade_no.clone(); // 提前克隆trade_no
                    if let Err(err) = self.process_fee_single_tx(req).await {
                        tracing::error!(trade_no=%trade_no, "[手续费归集] 处理单个手续费交易失败: {:?}", err);
                    }
                }
            }
            Err(err) => {
                tracing::error!("[手续费归集] 获取手续费交易记录列表失败: {:?}", err);
            }
        }
    }

    async fn process_fee_single_tx(&self, req: ApiFeeEntity) -> Result<(), ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "[手续费归集] 处理单个手续费交易");
        // check
        tracing::info!(trade_no=%req.trade_no, "[手续费归集] 验证交易数据完整性");
        if !self.check_digest(&req).await {
            tracing::error!(trade_no=%req.trade_no, "[手续费归集] 交易数据验证失败");
            return self
                .handle_fee_tx_failed(
                    &req.trade_no,
                    ServiceError::Parameter("validate failed".to_string()),
                )
                .await;
        }
        tracing::info!(trade_no=%req.trade_no, "[手续费归集] 交易数据验证通过");

        let from_addr = req.from_addr.clone();
        // 保存trade_no，因为req会被移动
        let trade_no = req.trade_no.clone();

        tracing::info!(trade_no=%trade_no, from_addr=%from_addr, "[手续费归集] 锁定发送账户");
        self.ctx.lock_account(&from_addr).await;

        tracing::info!(trade_no=%trade_no, "[手续费归集] 生成转账请求");
        let transfer_req_res = self.gen_transfer_req(&req).await;
        let result = match transfer_req_res {
            Ok(transfer_req) => {
                tracing::info!(trade_no=%trade_no, nonce=%transfer_req.nonce, "[手续费归集] 转账请求生成成功，准备发送交易");
                // 发交易
                let nonce = transfer_req.nonce;
                tracing::info!(trade_no=%trade_no, "[手续费归集] 调用转账接口发送交易");

                // 私钥缓存逻辑
                let from = req.from_addr.clone();
                let chain_code = req.chain_code.clone();
                let cache_key = (from.clone(), chain_code.clone());

                tracing::info!(trade_no=%trade_no, from=%from, chain_code=%chain_code, "[手续费归集] 检查私钥缓存");
                let private_key = if let Some(key) = self.private_key_cache.get(&cache_key) {
                    tracing::info!(trade_no=%trade_no, from=%from, chain_code=%chain_code, "[手续费归集] 从缓存中获取私钥");
                    key.clone()
                } else {
                    tracing::info!(trade_no=%trade_no, from=%from, chain_code=%chain_code, "[手续费归集] 从数据库获取私钥");
                    let password = transfer_req.password.clone();
                    let private_key = ApiAccountDomain::get_private_key(
                        from.as_str(),
                        chain_code.as_str(),
                        password.as_str(),
                    )
                    .await?;
                    tracing::info!(trade_no=%trade_no, from=%from, chain_code=%chain_code, "[手续费归集] 将私钥存入缓存");
                    self.private_key_cache.insert(cache_key, private_key.clone());
                    private_key
                };

                let tx_resp = ApiTransDomain::transfer(transfer_req, Some(private_key)).await;
                match tx_resp {
                    Ok(tx) => {
                        tracing::info!(trade_no=%trade_no, tx_hash=%tx.tx_hash, "[手续费归集] 交易发送成功");
                        // 保存需要在后续使用的字段
                        let from_addr_clone = from_addr.clone();
                        let trade_no_clone = trade_no.clone();

                        // 移动req的所有权到handle_fee_tx_success函数
                        let result = self.handle_fee_tx_success(req, tx, nonce).await;

                        // 使用克隆的字段解锁账户
                        tracing::info!(trade_no=%trade_no_clone, from_addr=%from_addr_clone, "[手续费归集] 解锁发送账户");
                        self.ctx.unlock_account(&from_addr_clone).await;

                        result
                    }
                    Err(err) => {
                        tracing::error!(trade_no=%trade_no, "[手续费归集] 交易发送失败: {}", err);
                        // 这里使用trade_no而不是&req.trade_no
                        self.handle_fee_tx_failed(&trade_no, err).await
                    }
                }
            }
            Err(err) => {
                tracing::error!(trade_no=%trade_no, "[手续费归集] 生成转账请求失败: {}", err);
                // 这里使用trade_no而不是&req.trade_no
                self.handle_fee_tx_failed(&trade_no, err).await
            }
        };

        // 只有当交易发送失败时才会执行到这里
        tracing::info!(trade_no=%trade_no, from_addr=%from_addr, "[手续费归集] 解锁发送账户");
        self.ctx.unlock_account(&from_addr).await;
        result
    }

    async fn check_digest(&self, req: &ApiFeeEntity) -> bool {
        tracing::debug!(trade_no=%req.trade_no, "[手续费归集] 验证交易摘要");
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let mut d = Decimal::from_str(req.value.as_str()).unwrap();
        d = d.normalize();
        let raw_data = req.from_addr.clone() + req.to_addr.as_str() + d.to_string().as_str() + sn;
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(&raw_data));
        let is_valid = req.validate == digest;
        tracing::debug!(trade_no=%req.trade_no, "[手续费归集] 摘要验证结果: {}", is_valid);
        is_valid
    }

    async fn get_eth_nonce(&self, from_addr: &str, chain_code: &str) -> Result<i64, ServiceError> {
        tracing::debug!(from_addr=%from_addr, chain_code=%chain_code, "[手续费归集] 获取以太坊类链的nonce值");
        match ApiNonceRepo::get_api_nonce(&self.pool, from_addr, chain_code).await {
            Ok(nonce) => {
                let new_nonce = nonce + 1;
                tracing::debug!(from_addr=%from_addr, chain_code=%chain_code, nonce=%new_nonce, "[手续费归集] 从数据库获取nonce并递增");
                Ok(new_nonce)
            }
            Err(_) => {
                tracing::debug!(from_addr=%from_addr, chain_code=%chain_code, "[手续费归集] 从数据库获取nonce失败，尝试从链上获取");
                let nonce = ApiTransDomain::nonce(from_addr, chain_code).await?;
                tracing::debug!(from_addr=%from_addr, chain_code=%chain_code, nonce=%nonce, "[手续费归集] 从链上获取nonce成功");
                Ok(nonce as i64)
            }
        }
    }

    async fn gen_transfer_req(&self, req: &ApiFeeEntity) -> Result<ApiTransferReq, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, chain_code=%req.chain_code, symbol=%req.symbol, "[手续费归集] 获取代币信息");
        let coin =
            ApiCoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_addr.clone()).await?;
        tracing::info!(trade_no=%req.trade_no, token_address=?coin.token_address, decimals=%coin.decimals, "[手续费归集] 代币信息获取成功");

        tracing::info!(trade_no=%req.trade_no, from_addr=%req.from_addr, to_addr=%req.to_addr, value=%req.value, "[手续费归集] 创建基础转账请求");
        let mut params =
            ApiBaseTransferReq::new(&req.from_addr, &req.to_addr, &req.value, &req.chain_code);
        let token_address = if coin.token_address.is_none() {
            None
        } else {
            let s = coin.token_address.unwrap();
            if s.is_empty() { None } else { Some(s) }
        };
        tracing::info!(trade_no=%req.trade_no, token_address=?token_address, "[手续费归集] 设置代币转账参数");
        params.with_token(token_address, coin.decimals, &coin.symbol);

        tracing::info!(trade_no=%req.trade_no, "[手续费归集] 获取钱包密码");
        let passwd = ApiWalletDomain::get_passwd().await?;

        let chain_code = req.chain_code.as_str();
        let chain_code: ChainCode = chain_code.try_into()?;
        tracing::info!(trade_no=%req.trade_no, chain_code=%chain_code, "[手续费归集] 根据链类型获取nonce值");
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
        tracing::info!(trade_no=%req.trade_no, nonce=%nonce, "[手续费归集] 转账请求生成完成");
        Ok(ApiTransferReq { base: params, password: passwd, nonce: nonce as u64 })
    }

    async fn handle_fee_tx_success(
        &self,
        req: ApiFeeEntity,
        tx: TransferResp,
        nonce: u64,
    ) -> Result<(), ServiceError> {
        tracing::info!(trade_no=%req.trade_no, tx_hash=%tx.tx_hash, "[手续费归集] 处理交易发送成功");
        let resource_consume = if tx.consumer.is_none() {
            "0".to_string()
        } else {
            tx.consumer.unwrap().energy_used.to_string()
        };
        tracing::info!(trade_no=%req.trade_no, resource_consume=%resource_consume, fee=%tx.fee, "[手续费归集] 交易消耗资源和手续费");

        let res = if req.chain_code == ChainCode::Ethereum.to_string()
            || req.chain_code == ChainCode::BnbSmartChain.to_string()
        {
            tracing::info!(trade_no=%req.trade_no, "[手续费归集] 更新以太坊/BBSC链交易状态和nonce");
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
            tracing::info!(trade_no=%req.trade_no, "[手续费归集] 更新其他链交易状态");
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
                tracing::info!(trade_no=%req.trade_no, "[手续费归集] 交易状态更新成功");
                // 上报交易不影响交易偏移量计算
                tracing::info!(trade_no=%req.trade_no, "[手续费归集] 发送交易报告请求");
                self.report_tx
                    .send(ProcessFeeTxReportCommand::Tx(req.trade_no.to_string()))
                    .await
                    .map_err(|e| {
                        ServiceError::System(SystemError::ChannelSendFailed(e.to_string()))
                    })?;
                Ok(())
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[手续费归集] 更新交易状态失败: {}", err);
                Err(err.into())
            }
        }
    }

    async fn handle_fee_tx_failed(
        &self,
        trade_no: &str,
        err: ServiceError,
    ) -> Result<(), ServiceError> {
        tracing::error!(trade_no=%trade_no, "[手续费归集] 处理交易发送失败: {}", err);
        let res = ApiFeeRepo::update_api_fee_status_and_err(
            &self.pool,
            trade_no,
            ApiFeeStatus::SendingTxFailed,
            101,
            &err.to_string(),
        )
        .await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%trade_no, "[手续费归集] 交易失败状态更新成功");
                // 上报交易不影响交易偏移量计算
                tracing::info!(trade_no=%trade_no, "[手续费归集] 发送交易报告请求");
                self.report_tx
                    .send(ProcessFeeTxReportCommand::Tx(trade_no.to_string()))
                    .await
                    .map_err(|e| {
                        ServiceError::System(SystemError::ChannelSendFailed(e.to_string()))
                    })?;
                Ok(())
            }
            Err(err) => {
                tracing::error!(trade_no=%trade_no, "[手续费归集] 更新交易失败状态失败: {}", err);
                Err(err.into())
            }
        }
    }
}
