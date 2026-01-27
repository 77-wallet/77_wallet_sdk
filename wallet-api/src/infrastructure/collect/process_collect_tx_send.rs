use crate::{
    domain::{
        api_wallet::{
            adapter_factory::ApiChainAdapterFactory, chain::ApiChainTransDomain,
            coin::ApiCoinDomain, strategy::StrategyDomain, trans::ApiTransDomain,
            wallet::ApiWalletDomain,
        },
        chain::TransferResp,
    },
    error::{
        business::api_wallet::{ApiWalletError, trans::TransError},
        service::ServiceError,
        system::SystemError,
    },
    infrastructure::collect::command::{ProcessCollectTxCommand, ProcessCollectTxReportCommand},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
    response_vo::{CommonFeeDetails, EthereumFeeDetails, FeeDetailsVo, TronFeeDetails},
};
use dashmap::{DashMap, DashSet};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use std::{
    str::FromStr,
    sync::{Arc, Weak},
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    CollectDbPool, CoreDbPool,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::{
        account::ApiAccountRepo, collect::ApiCollectRepo, nonce::ApiNonceRepo,
        wallet::ApiWalletRepo,
    },
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::{
    strategy::ChainConfig, transaction::ServiceFeeUploadReq,
};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::{conversion, unit};

pub struct AddressLockManager {
    locks: DashMap<String, Weak<Semaphore>>,
}

impl AddressLockManager {
    pub fn new() -> Self {
        Self { locks: DashMap::new() }
    }

    /// 获取某个账户的独占执行权
    ///
    /// 返回的 `OwnedSemaphorePermit`：
    /// - 生命周期即锁生命周期
    /// - drop 自动释放
    pub async fn acquire(&self, account: &str) -> Result<OwnedSemaphorePermit, ServiceError> {
        let sem = self.get_or_create_semaphore(account);
        sem.acquire_owned().await.map_err(|_| ServiceError::System(SystemError::SemaphoreClosed))
    }

    fn get_or_create_semaphore(&self, account: &str) -> Arc<Semaphore> {
        use dashmap::mapref::entry::Entry;

        match self.locks.entry(account.to_string()) {
            Entry::Occupied(mut e) => {
                if let Some(sem) = e.get().upgrade() {
                    sem
                } else {
                    let sem = Arc::new(Semaphore::new(1));
                    e.insert(Arc::downgrade(&sem));
                    sem
                }
            }
            Entry::Vacant(e) => {
                let sem = Arc::new(Semaphore::new(1));
                e.insert(Arc::downgrade(&sem));
                sem
            }
        }
    }
}

// Lock order (MUST NOT change):
// 1. address semaphore
// 2. global semaphore
#[derive(Clone)]
struct CollectTxWorkerCtx {
    core_pool: CoreDbPool,
    api_fund_pool: CollectDbPool,
    address_locks: Arc<AddressLockManager>,
    global_sem: Arc<Semaphore>,
    processing_trade: Arc<DashSet<String>>,
    batch_running: Arc<Semaphore>,
    report_tx: mpsc::Sender<ProcessCollectTxReportCommand>,
}

pub(super) struct ProcessCollectTx {
    worker_ctx: CollectTxWorkerCtx,
    shutdown_rx: broadcast::Receiver<()>,
    tx_rx: mpsc::Receiver<ProcessCollectTxCommand>,
    // address_locks: Arc<AddressLockManager>,
    // report_tx: mpsc::Sender<ProcessCollectTxReportCommand>,
}

impl ProcessCollectTx {
    pub(super) fn new(
        core_pool: CoreDbPool,
        pool: CollectDbPool,
        shutdown_rx: broadcast::Receiver<()>,
        tx_rx: mpsc::Receiver<ProcessCollectTxCommand>,
        report_tx: mpsc::Sender<ProcessCollectTxReportCommand>,
    ) -> Self {
        let worker_ctx = CollectTxWorkerCtx {
            core_pool,
            api_fund_pool: pool.clone(),
            address_locks: Arc::new(AddressLockManager::new()),
            global_sem: Arc::new(Semaphore::new(32)), // 比 report 小一点
            processing_trade: Arc::new(DashSet::new()),
            report_tx: report_tx.clone(),
            batch_running: Arc::new(Semaphore::new(1)),
        };

        Self { shutdown_rx, tx_rx, worker_ctx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!("starting process collect -------------------------------");
        self.run_with_err().await;
        tracing::info!("closing process collect tx ------------------------------- end");
    }

    async fn run_with_err(&mut self) {
        tracing::info!("collect_tx:send: 启动归集交易处理循环");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                tracing::warn!("collect_tx:send: 共享密钥未设置，等待10秒后重试");
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("collect_tx:send: 接收到关闭信号，退出处理循环");
                    break;
                }
                msg = self.tx_rx.recv() => {
                    if let Some(cmd) = msg {
                        match cmd {
                            ProcessCollectTxCommand::Tx(trade_no) => {
                                tracing::info!(trade_no=%trade_no, "collect_tx:send: 接收到单个交易处理请求");
                                self.spawn_single(&trade_no);
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    tracing::info!("collect_tx:send: 执行定时批量处理归集交易");
                    self.spawn_batch()
                }
            }
        }
    }

    fn spawn_single(&self, trade_no: &str) {
        let ctx = self.worker_ctx.clone();
        let trade_no = trade_no.to_string();

        tokio::spawn(async move {
            let req = match ApiCollectRepo::get_api_collect_by_trade_no_status(
                &ctx.api_fund_pool,
                &trade_no,
                &[ApiCollectStatus::Init],
            )
            .await
            {
                Ok(res) => res,
                Err(err) => {
                    tracing::warn!(trade_no=%trade_no, "process collect tx not found: {}", err);
                    return;
                }
            };
            if !ctx.processing_trade.insert(req.trade_no.clone()) {
                tracing::warn!(trade_no=%req.trade_no, "collect tx already processing, skip");
                return;
            }
            let _guard = TradeGuard::new(&req.trade_no, ctx.processing_trade.clone());

            if let Err(e) = Self::process_collect_single_tx(ctx, req).await {
                tracing::error!(trade_no=%trade_no, "collect_tx:send: 处理单个归集交易失败: {}", e);
            }
        });
    }

    fn spawn_batch(&self) {
        // batch 级互斥：只在这里拿一次
        let permit = match self.worker_ctx.batch_running.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                tracing::info!("collect_tx:send: batch 正在运行，跳过本轮");
                return;
            }
        };

        tracing::info!("collect_tx:send: 查询待处理的归集交易");
        let ctx = self.worker_ctx.clone();

        tokio::spawn(async move {
            let _batch_guard = permit;
            // 获取交易这里有问题
            let res = ApiCollectRepo::page_api_collect_with_status(
                &ctx.api_fund_pool,
                0,
                1000,
                &[ApiCollectStatus::Init],
            )
            .await;
            let (_, collect_txs) = match res {
                Ok(v) => v,
                Err(err) => {
                    tracing::warn!("process_collect_tx_send 查询待处理归集交易失败: {}", err);
                    return;
                }
            };
            tracing::info!("collect_tx:send: 找到 {} 笔待处理的归集交易", collect_txs.len());
            for req in collect_txs {
                let ctx = ctx.clone();
                let trade_no = req.trade_no.clone(); // 提前克隆trade_no
                if !ctx.processing_trade.insert(trade_no.clone()) {
                    continue;
                }
                tokio::spawn(async move {
                    let _guard = TradeGuard::new(&trade_no, ctx.processing_trade.clone());
                    if let Err(err) = Self::process_collect_single_tx(ctx, req).await {
                        tracing::error!(trade_no=%trade_no, "collect_tx:send: 处理单个归集交易失败: {}", err);
                    }
                });
            }
        });
    }

    async fn process_collect_single_tx(
        worker_ctx: CollectTxWorkerCtx,
        mut req: ApiCollectEntity,
    ) -> Result<(), ServiceError> {
        // 终态检查：终态订单不得重复处理
        if req.status.is_terminal() {
            tracing::warn!(
                trade_no = %req.trade_no,
                status = ?req.status,
                "collect_tx:send: 订单已处于终态，跳过执行"
            );
            return Ok(());
        }

        let _addr_guard = worker_ctx.address_locks.acquire(&req.from_addr).await?;
        let _global_guard = worker_ctx
            .global_sem
            .acquire()
            .await
            .map_err(|_| ServiceError::System(SystemError::SemaphoreClosed))?;

        // ⚠️ Step 0: 已生成raw_tx的交易优先检查链上状态
        if let Some(tx_hash) = req.tx_hash.as_deref() {
            tracing::info!(trade_no=%req.trade_no, "collect_tx: 检测到已有raw_tx和tx_hash，执行恢复检查");

            // 使用通用的交易恢复逻辑
            match ApiTransDomain::process_recovered_tx(
                &req.chain_code,
                &req.from_addr,
                tx_hash,
                req.nonce,
                &req.transaction_fee,
            )
            .await
            {
                Ok(Some(tx_resp)) => {
                    // 保存nonce值，因为req将被移动
                    let nonce = req.nonce as u64;
                    return Self::handle_collect_tx_success(&worker_ctx, req, tx_resp, nonce).await;
                }
                Ok(None) => {
                    return Ok(()); // 容错，下轮再查
                }
                Err(err) => {
                    return Self::handle_collect_tx_failed(&worker_ctx, &req.trade_no, err).await;
                }
            }
        }

        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 开始处理归集交易, from={}, to={}, value={}, chain={}, symbol={}", 
            req.from_addr, req.to_addr, req.value, req.chain_code, req.symbol);
        // 解析执行地址 - 在执行期解析，支持重试
        let exec_to_addr = Self::resolve_collect_to_addr(&worker_ctx, &req).await?;
        if req.to_addr.is_empty() {
            req.to_addr = exec_to_addr.clone();
            // 更新数据库中的to_addr
            ApiCollectRepo::update_api_collect_to_addr(
                &worker_ctx.api_fund_pool,
                &req.trade_no,
                &exec_to_addr,
            )
            .await?;
        }

        // 检查手续费
        let check_res = worker_ctx.check_fee(&req).await;
        let trade_no = &req.trade_no;
        match check_res {
            Ok(pass) => {
                if !pass {
                    tracing::info!(trade_no=%trade_no, "collect_tx:send: 手续费不足，已请求补充");
                    return Ok(());
                }
                tracing::info!(trade_no=%trade_no, "collect_tx:send: 手续费检查通过");
            }
            Err(err) => {
                tracing::error!(trade_no=%trade_no, "collect_tx:send: 手续费检查失败: {}", err);
                return Self::handle_collect_tx_failed(&worker_ctx, trade_no, err).await;
            }
        }

        // 检查交易摘要 - 仍然使用 req.to_addr（原始输入）
        if !Self::check_digest(&req).await {
            tracing::error!(trade_no=%trade_no, "collect_tx:send: 交易摘要验证失败");
            return Self::handle_collect_tx_failed(
                &worker_ctx,
                trade_no,
                ServiceError::Business(
                    ApiWalletError::Trans(TransError::TransactionDigestVerificationFailed).into(),
                ),
            )
            .await;
        }
        tracing::info!(trade_no=%trade_no, "collect_tx:send: 交易摘要验证通过");

        // 生成转账请求 - 使用解析后的执行地址
        let transfer_req_res = Self::gen_transfer_req(&worker_ctx, &req, &exec_to_addr).await;
        match transfer_req_res {
            Ok(transfer_req) => {
                tracing::info!(trade_no=%trade_no, "collect_tx:send: 生成转账请求成功，准备发送交易");

                // 发送交易
                let nonce = transfer_req.nonce;
                tracing::info!(trade_no=%trade_no, "collect_tx:send: 开始发送归集交易, nonce={}", nonce);

                // 通过Context获取Handles实例，然后获取私钥管理器
                let handles = crate::context::get_context()?.get_handles_arc().await?;
                let private_key_manager = handles.get_global_private_key_manager();
                let private_key =
                    private_key_manager.get_private_key(&req.from_addr, &req.chain_code).await?;
                tracing::info!(trade_no=%trade_no, "collect_tx:send: 从私钥管理器获取私钥");
                // 将私钥字符串转换为ChainPrivateKey类型
                let (tx_hash, raw_tx, fee) = match ApiTransDomain::build_transfer_raw(
                    transfer_req,
                    Some(private_key),
                )
                .await
                {
                    Ok((tx_hash, raw_tx, fee)) => (tx_hash, raw_tx, fee),
                    Err(err) => {
                        tracing::error!(trade_no=%trade_no, "collect_tx:send: 构建转账原始交易失败: {}", err);
                        return Self::handle_collect_tx_failed(
                            &worker_ctx,
                            trade_no,
                            ServiceError::Business(
                                ApiWalletError::Trans(TransError::BuildWithdrawTransactionFailed(
                                    err.to_string(),
                                ))
                                .into(),
                            ),
                        )
                        .await;
                    }
                };
                tracing::info!(trade_no=%trade_no, "collect_tx:send: 构建转账原始交易成功, tx_hash={}, fee={}", tx_hash, fee);

                // Step 2: 立即将tx_hash和raw_tx存储到数据库
                tracing::info!(trade_no=%trade_no, "collect_tx:send: 开始将tx_hash和raw_tx存储到数据库");
                // 将RawTx转换为字符串进行存储

                let raw_tx_str = wallet_utils::serde_func::serde_to_string(&raw_tx)?;
                let update_res = ApiCollectRepo::update_after_build(
                    &worker_ctx.api_fund_pool,
                    &req.trade_no,
                    &tx_hash,
                    &raw_tx_str,
                    &fee,
                )
                .await;

                if let Err(err) = update_res {
                    tracing::error!(trade_no=%trade_no, "collect_tx:send: 将tx_hash和raw_tx存储到数据库失败: {}", err);
                    return Self::handle_collect_tx_failed(
                        &worker_ctx,
                        trade_no,
                        ServiceError::Database(err.into()),
                    )
                    .await;
                }
                tracing::info!(trade_no=%trade_no, "collect_tx:send: tx_hash和raw_tx存储到数据库成功");

                // Step 3: 广播交易
                tracing::info!(trade_no=%trade_no, "collect_tx:send: 开始广播交易");
                let tx_resp = ApiTransDomain::broadcast_transfer(&req.chain_code, raw_tx).await;
                // let tx_resp = ApiTransDomain::transfer(transfer_req, Some(private_key)).await;
                match tx_resp {
                    Ok(Some(tx)) => {
                        tracing::info!(trade_no=%trade_no, "collect_tx:send: 交易广播成功, tx_hash={}", tx.tx_hash);
                        // 广播成功后，更新交易状态
                        return Self::handle_collect_tx_success(&worker_ctx, req, tx, nonce).await;
                    }
                    Ok(None) => {
                        tracing::info!(trade_no=%trade_no, "collect_tx:send: 交易广播结果不确定");
                        return Ok(());
                    }
                    Err(err) => {
                        tracing::error!(trade_no=%trade_no, "collect_tx:send: 交易广播失败: {}", err);
                        return Self::handle_collect_tx_failed(&worker_ctx, trade_no, err).await;
                    }
                }
            }
            Err(err) => {
                tracing::error!(trade_no=%trade_no, "collect_tx:send: 生成转账请求失败: {}", err);
                return Self::handle_collect_tx_failed(&worker_ctx, trade_no, err).await;
            }
        }
    }

    async fn check_digest(req: &ApiCollectEntity) -> bool {
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 开始验证交易摘要");
        // check digest
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let mut d = Decimal::from_str(req.value.as_str()).unwrap();
        d = d.normalize();
        // let raw_data = req.from_addr.clone() + req.to_addr.as_str() + d.to_string().as_str() + sn;
        // ⚠️ 这里必须用后端给的空字符串的to_addr，不能用查询策略解析的地址
        let raw_data = req.from_addr.clone() + "" + d.to_string().as_str() + sn;
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(&raw_data));

        let is_valid = req.validate == digest;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 交易摘要验证完成, 结果: {}", is_valid);
        is_valid
    }

    async fn get_eth_nonce(
        pool: &CollectDbPool,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<i64, ServiceError> {
        tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "collect_tx:send: 获取以太坊nonce");
        match ApiNonceRepo::get_api_nonce(&pool, from_addr, chain_code).await {
            Ok(nonce) => {
                let next_nonce = nonce + 1;
                tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "collect_tx:send: 从本地缓存获取nonce: {}, 下一个nonce: {}", nonce, next_nonce);
                Ok(next_nonce)
            }
            Err(_) => {
                tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "collect_tx:send: 本地缓存未找到nonce，从链上获取");
                let nonce = ApiTransDomain::nonce(from_addr, chain_code).await?;
                tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "collect_tx:send: 从链上获取nonce: {}", nonce);
                Ok(nonce as i64)
            }
        }
    }

    async fn resolve_collect_to_addr(
        worker_ctx: &CollectTxWorkerCtx,
        req: &ApiCollectEntity,
    ) -> Result<String, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 开始解析执行地址");

        // 1. 根据from_addr + chain_code查询account
        let account = match ApiAccountRepo::find_one_by_address_chain_code(
            &req.from_addr,
            &req.chain_code,
            &worker_ctx.core_pool,
        )
        .await?
        {
            Some(account) => account,
            None => {
                tracing::warn!(trade_no=%req.trade_no, "collect_tx:send: 账户不存在, from_addr={}, chain_code={}", req.from_addr, req.chain_code);
                return Err(ServiceError::Business(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::Account(
                            crate::error::business::api_wallet::account::AccountError::NotFound,
                        ),
                    ),
                ));
            }
        };

        // 2. 根据account.wallet_address查询wallet
        let wallet = match ApiWalletRepo::find_by_address(
            &worker_ctx.core_pool.clone(),
            &account.wallet_address,
        )
        .await?
        {
            Some(wallet) => wallet,
            None => {
                tracing::warn!(trade_no=%req.trade_no, "collect_tx:send: 钱包不存在, wallet_address={}", account.wallet_address);
                return Err(ServiceError::Business(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::Wallet(
                            crate::error::business::api_wallet::wallet::WalletError::NotFound
                                .into(),
                        ),
                    ),
                ));
            }
        };

        // 3. 查询用户归集策略
        let strategy = StrategyDomain::query_collect_strategy(&wallet.uid).await?;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 获取归集策略成功, 包含 {} 条链配置", strategy.chain_configs.len());

        // 4. 根据chain_code查询链配置
        let chain_config = match strategy
            .chain_configs
            .into_iter()
            .find(|config| config.chain_code == req.chain_code)
        {
            Some(config) => config,
            None => {
                tracing::error!(trade_no=%req.trade_no, "collect_tx:send: 未找到对应的链配置, chain_code={}", req.chain_code);
                return Err(ServiceError::Business(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::ChainConfigNotFound(
                            req.chain_code.clone(),
                        ),
                    ),
                ));
            }
        };

        // 5. 根据risk_addr决定normal/risk地址
        // risk_addr: 1 正常地址，2 风险地址
        let exec_to_addr = match req.risk_addr {
            1 => chain_config.normal_address.address.clone(),
            2 => chain_config.risk_address.address.clone(),
            _ => {
                tracing::error!(trade_no=%req.trade_no, "collect_tx:send: 非法 risk_addr={}", req.risk_addr);
                return Err(ServiceError::Business(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::Strategy(
                            crate::error::business::api_wallet::strategy::StrategyError::StatusNotMatched
                        )
                    )
                ));
            }
        };

        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 解析执行地址成功, exec_to_addr={}", exec_to_addr);
        Ok(exec_to_addr)
    }

    async fn resolve_withdraw_from_addr(
        worker_ctx: &CollectTxWorkerCtx,
        req: &ApiCollectEntity,
    ) -> Result<String, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 开始解析提币地址");
        // 1. 根据from_addr + chain_code查询account
        let account = match ApiAccountRepo::find_one_by_address_chain_code(
            &req.from_addr,
            &req.chain_code,
            &worker_ctx.core_pool,
        )
        .await?
        {
            Some(account) => account,
            None => {
                tracing::warn!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 提币账户不存在, from_addr={}, chain_code={}", req.from_addr, req.chain_code);
                return Err(ServiceError::Business(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::Account(
                            crate::error::business::api_wallet::account::AccountError::NotFound,
                        ),
                    ),
                ));
            }
        };

        // 2. 根据account.wallet_address查询wallet
        let wallet = match ApiWalletRepo::find_by_address(
            &worker_ctx.core_pool,
            &account.wallet_address,
        )
        .await?
        {
            Some(wallet) => wallet,
            None => {
                tracing::warn!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 钱包不存在, wallet_address={}", account.wallet_address);
                return Err(ServiceError::Business(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::Wallet(
                            crate::error::business::api_wallet::wallet::WalletError::NotFound
                                .into(),
                        ),
                    ),
                ));
            }
        };
        let Some(bind_address) = wallet.binding_address else {
            tracing::warn!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 钱包未绑定地址, wallet_address={}", account.wallet_address);
            return Err(ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::ApiWalletError::Wallet(
                        crate::error::business::api_wallet::wallet::WalletError::SubAccountWalletNotBoundWithdrawalWalletAddress
                            .into(),
                    ),
                ),
            ));
        };

        let Some(withdraw_wallet) =
            ApiWalletRepo::find_by_address(&worker_ctx.core_pool, &bind_address).await?
        else {
            tracing::warn!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 出款钱包不存在, bind_address={}", bind_address);
            return Err(ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::Wallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                ),
            )));
        };

        // 3. 查询用户提币策略
        let strategy = StrategyDomain::query_withdraw_strategy(&withdraw_wallet.uid).await?;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 获取提现策略成功, 包含 {} 条链配置", strategy.chain_configs.len());

        // 4. 根据chain_code查询链配置
        let chain_config = match strategy
            .chain_configs
            .into_iter()
            .find(|config| config.chain_code == req.chain_code)
        {
            Some(config) => config,
            None => {
                tracing::error!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 未找到对应的链配置, chain_code={}", req.chain_code);
                return Err(ServiceError::Business(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::ChainConfigNotFound(
                            req.chain_code.clone(),
                        ),
                    ),
                ));
            }
        };

        // 5. 根据risk_addr决定normal/risk地址
        // risk_addr: 1 正常地址，2 风险地址
        let exec_from_addr = chain_config.normal_address.address;

        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: resolve_withdraw_from_addr: 解析执行地址成功, exec_to_addr={}", exec_from_addr);
        Ok(exec_from_addr)
    }

    async fn gen_transfer_req(
        worker_ctx: &CollectTxWorkerCtx,
        req: &ApiCollectEntity,
        exec_to_addr: &str,
    ) -> Result<ApiTransferReq, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 开始生成转账请求, exec_to_addr={}", exec_to_addr);

        // 获取币种信息
        let coin =
            ApiCoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_addr.clone()).await?;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 获取币种信息成功, symbol={}, token_address={:?}, decimals={}", 
            coin.symbol, coin.token_address, coin.decimals);

        // 创建基础转账请求 - 使用exec_to_addr而非req.to_addr
        let mut params =
            ApiBaseTransferReq::new(&req.from_addr, exec_to_addr, &req.value, &req.chain_code);
        let token_address = if coin.token_address.is_none() {
            None
        } else {
            let s = coin.token_address.unwrap();
            if s.is_empty() { None } else { Some(s) }
        };
        params.with_token(token_address, coin.decimals, &coin.symbol);
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 创建基础转账请求成功");

        // 获取钱包密码
        let passwd = ApiWalletDomain::get_passwd().await?;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 获取钱包密码成功");

        // 计算nonce
        let chain_code = req.chain_code.as_str();
        let chain_code: ChainCode = chain_code.try_into()?;
        let nonce: i64 = match chain_code {
            ChainCode::Tron => 0,
            ChainCode::Bitcoin => 0,
            ChainCode::Solana => 0,
            ChainCode::Ethereum => {
                Self::get_eth_nonce(&worker_ctx.api_fund_pool, &req.from_addr, &req.chain_code)
                    .await?
            }
            ChainCode::BnbSmartChain => {
                Self::get_eth_nonce(&worker_ctx.api_fund_pool, &req.from_addr, &req.chain_code)
                    .await?
            }
            ChainCode::Litecoin => 0,
            ChainCode::Dogcoin => 0,
            ChainCode::Sui => 0,
            ChainCode::Ton => 0,
        };
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 计算nonce成功, nonce={}", nonce);

        let transfer_req = ApiTransferReq { base: params, password: passwd, nonce: nonce as u64 };
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 生成转账请求成功");
        Ok(transfer_req)
    }

    async fn handle_collect_tx_success(
        worker_ctx: &CollectTxWorkerCtx,
        req: ApiCollectEntity,
        tx: TransferResp,
        nonce: u64,
    ) -> Result<(), ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 处理交易成功结果");

        let resource_consume = if let Some(consumer) = tx.consumer {
            consumer.energy_used.to_string()
        } else {
            "0".to_string()
        };

        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 交易资源消耗: {}, 手续费: {}", resource_consume, tx.fee);

        // 更新交易状态
        let res = if req.chain_code == ChainCode::Ethereum.to_string()
            || req.chain_code == ChainCode::BnbSmartChain.to_string()
        {
            tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 更新以太坊/BSC交易状态，包含nonce");
            ApiCollectRepo::update_api_collect_tx_status_nonce(
                &worker_ctx.api_fund_pool,
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
            tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 更新非以太坊/BSC交易状态");
            ApiCollectRepo::update_api_collect_tx_status(
                &worker_ctx.api_fund_pool,
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
                tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 更新交易状态成功，交易已发送");
                // 上报交易不影响交易偏移量计算
                tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 准备上报交易结果");
                worker_ctx
                    .report_tx
                    .send(ProcessCollectTxReportCommand::Tx(req.trade_no.to_string()))
                    .await
                    .map_err(|e| {
                        ServiceError::System(SystemError::ChannelSendFailed(e.to_string()))
                    })?;
                tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 交易上报完成");
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "collect_tx:send: 更新交易状态失败: {}", err);
            }
        }
        Ok(())
    }

    async fn handle_collect_tx_failed(
        worker_ctx: &CollectTxWorkerCtx,
        trade_no: &str,
        err: ServiceError,
    ) -> Result<(), ServiceError> {
        tracing::info!(trade_no=%trade_no, "collect_tx:send: 处理交易失败结果, 错误: {}", err);

        // 更新失败状态
        let res = ApiCollectRepo::update_api_collect_status_and_err(
            &worker_ctx.api_fund_pool,
            trade_no,
            ApiCollectStatus::SendingTxFailed,
            101,
            &err.to_string(),
        )
        .await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%trade_no, "collect_tx:send: 更新交易状态为失败成功");
                // 上报交易不影响交易偏移量计算
                tracing::info!(trade_no=%trade_no, "collect_tx:send: 准备上报失败交易");
                worker_ctx
                    .report_tx
                    .send(ProcessCollectTxReportCommand::Tx(trade_no.to_string()))
                    .await
                    .map_err(|e| {
                        ServiceError::System(SystemError::ChannelSendFailed(e.to_string()))
                    })?;
                tracing::info!(trade_no=%trade_no, "collect_tx:send: 失败交易上报完成");
            }
            Err(err) => {
                tracing::error!(trade_no=%trade_no, "collect_tx:send: 更新失败状态失败: {}", err);
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
impl CheckFee for CollectTxWorkerCtx {
    async fn check_fee(&self, req: &ApiCollectEntity) -> Result<bool, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 开始检查手续费, 发送方={}, 接收方={}, 金额={}, 代币地址={:?}", 
            req.from_addr, req.to_addr, req.value, req.token_addr);

        // 查询主币信息
        let chain_code: ChainCode = req.chain_code.as_str().try_into()?;
        let main_coin = ApiChainTransDomain::main_coin(&req.chain_code).await?;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 主币信息: 币种={}, 小数位数={}", main_coin.symbol, main_coin.decimals);

        // 确定代币信息
        let (token_symbol, token, token_decimals) = if let Some(token) = req.token_addr.clone() {
            if token.is_empty() {
                (main_coin.symbol.clone(), None, main_coin.decimals)
            } else {
                let token_coin =
                    ApiCoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_addr.clone())
                        .await?;
                tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 代币信息: 币种={}, 代币地址={:?}, 小数位数={}", 
                    token_coin.symbol, token_coin.token_address, token_coin.decimals);
                (token_coin.symbol, token_coin.token_address, token_coin.decimals)
            }
        } else {
            (main_coin.symbol.clone(), None, main_coin.decimals)
        };

        // 估算手续费
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 开始估算手续费");
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
        let fee = conversion::decimal_from_str(&fee_str)?;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 估算手续费完成: {}", fee_str);

        // 查询资产主币余额
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 查询主币余额");
        let balance =
            self.query_balance(&req.from_addr, chain_code, None, main_coin.decimals).await?;
        let balance = conversion::decimal_from_str(&balance)?;
        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 主币余额查询完成: {}", balance);

        // 计算需要的总金额
        let need = if req.token_addr.is_some() {
            // 代币交易只需要手续费
            tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 代币交易，只需要手续费");
            fee
        } else {
            // 主币交易需要手续费+转账金额
            let value = conversion::decimal_from_str(&req.value)?;
            tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 主币交易，需要手续费+转账金额, 转账金额={}", value);
            fee + value
        };

        tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 手续费检查结果 - 可用余额: {}, 需要金额: {}, 手续费: {}", balance, need, fee);

        // 如果手续费不足，则从其他地址转入手续费费用
        if fee > Decimal::from(0) && balance < need {
            tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 手续费不足，需要请求补充");

            // 查询策略
            tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 查询归集策略");
            // let chain_config = self.get_collect_config(&req.uid, &req.chain_code).await?;
            // tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 获取归集策略成功, 正常地址: {}", chain_config.normal_address.address);

            // 计算需要补充的手续费
            let mut fee_to_upload = if let Some(f) = fee.to_f64() { f } else { 0.0 };
            if chain_code == ChainCode::Ethereum || chain_code == ChainCode::BnbSmartChain {
                fee_to_upload = fee_to_upload * 2.0;
                tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 以太坊/BSC网络，手续费翻倍: {}", fee_to_upload);
            }

            // 上传手续费记录
            let exec_from_addr = ProcessCollectTx::resolve_withdraw_from_addr(self, &req).await?;
            let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            let upload_req = ServiceFeeUploadReq::new(
                &req.trade_no,
                &req.chain_code,
                &main_coin.symbol,
                "",
                &exec_from_addr,
                &req.from_addr,
                fee_to_upload,
            );

            tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 上传手续费记录");
            backend_api.upload_service_fee_record(&upload_req).await?;
            tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 上传手续费记录成功");

            // 更新交易状态为余额不足
            tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 更新交易状态为余额不足");
            ApiCollectRepo::update_api_collect_status_and_err(
                &self.api_fund_pool,
                &req.trade_no,
                ApiCollectStatus::InsufficientBalance,
                102,
                "insufficient balance",
            )
            .await?;
            tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 更新交易状态完成");

            Ok(false)
        } else {
            tracing::info!(trade_no=%req.trade_no, "collect_tx:send: 手续费充足，继续交易");
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
            "collect_tx:send: 查询余额");

        // Log token_address before moving it to adapter.balance
        let token_address_log = token_address.clone();
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter(&chain_code.to_string()).await?;
        let balance = adapter.balance(&owner_address, token_address).await?;
        let amount = unit::format_to_string(balance, decimals)?;

        tracing::info!(owner_address=%owner_address, chain_code=%chain_code.to_string(), token_address=%token_address_log.as_deref().unwrap_or(""), 
            "collect_tx:send: 查询余额完成: {}", amount);
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
        let start_time = std::time::Instant::now();
        tracing::info!(from=%from, to=%to, value=%value, chain_code=%chain_code.to_string(), symbol=%symbol,
            main_symbol=%main_symbol, token_address=%token_address.as_deref().unwrap_or(""), 
            "collect_tx:send: 估算交易手续费开始");

        let adapter_start = std::time::Instant::now();
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter(&chain_code.to_string()).await?;
        tracing::info!(chain_code=%chain_code.to_string(), duration_ms=%adapter_start.elapsed().as_millis(), "collect_tx:send: 获取适配器完成");

        let params_start = std::time::Instant::now();
        let mut params = ApiBaseTransferReq::new(from, to, value, &chain_code.to_string());
        params.with_token(token_address, decimals, symbol);
        tracing::info!(chain_code=%chain_code.to_string(), duration_ms=%params_start.elapsed().as_millis(), "collect_tx:send: 构建请求参数完成");

        let estimate_start = std::time::Instant::now();
        let fee = adapter.estimate_fee(params, main_symbol).await?;
        tracing::info!(chain_code=%chain_code.to_string(), duration_ms=%estimate_start.elapsed().as_millis(), "collect_tx:send: 调用estimate_fee完成");

        let parse_start = std::time::Instant::now();
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
        tracing::info!(chain_code=%chain_code.to_string(), duration_ms=%parse_start.elapsed().as_millis(), "collect_tx:send: 解析手续费结果完成");

        tracing::info!(from=%from, to=%to, chain_code=%chain_code.to_string(), total_duration_ms=%start_time.elapsed().as_millis(), "collect_tx:send: 估算手续费完成: {}", amount);
        Ok(amount)
    }

    async fn get_collect_config(
        &self,
        uid: &str,
        chain_code: &str,
    ) -> Result<ChainConfig, ServiceError> {
        tracing::info!(uid=%uid, chain_code=%chain_code, "collect_tx:send: 查询归集策略");

        // 查询策略
        let strategy = StrategyDomain::query_collect_strategy(uid).await?;

        tracing::info!(uid=%uid, "collect_tx:send: 获取归集策略成功，包含 {} 条链配置", strategy.chain_configs.len());

        let Some(chain_config) =
            strategy.chain_configs.into_iter().find(|config| config.chain_code == chain_code)
        else {
            tracing::error!(uid=%uid, chain_code=%chain_code, "collect_tx:send: 未找到对应的链配置");
            return Err(crate::error::business::BusinessError::ApiWallet(
                ApiWalletError::ChainConfigNotFound(chain_code.to_owned()),
            )
            .into());
        };

        tracing::info!(uid=%uid, chain_code=%chain_code, "collect_tx:send: 找到链配置, normal_address={}", chain_config.normal_address.address);
        Ok(chain_config)
    }
}

pub(crate) struct TradeGuard {
    trade_no: String,
    set: Arc<DashSet<String>>,
}

impl TradeGuard {
    pub(crate) fn new(trade_no: &str, set: Arc<DashSet<String>>) -> Self {
        Self { trade_no: trade_no.to_string(), set }
    }
}

impl Drop for TradeGuard {
    fn drop(&mut self) {
        self.set.remove(&self.trade_no);
    }
}
