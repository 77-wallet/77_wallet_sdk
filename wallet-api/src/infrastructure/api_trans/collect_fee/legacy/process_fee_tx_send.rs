// legacy collect fee transaction send worker.
#![allow(deprecated)]

/// FeeTxSupervisor (你现在的 ProcessFeeTx)
/// │
/// ├─ BatchActor       (interval + batch_running)
/// ├─ SingleTxActor    (spawn_single)
/// ├─ AddressActor[x]  (AddressLockManager)
/// ├─ GlobalLimiter    (global_sem)
/// └─ TradeRegistry    (processing_trade)
use crate::{
    context::Context,
    domain::{
        api_wallet::{coin::ApiCoinDomain, trans::ApiTransDomain, wallet::ApiWalletDomain},
        chain::TransferResp,
    },
    error::{
        business::api_wallet::{ApiWalletError, trans::TransError},
        service::ServiceError,
        system::SystemError,
    },
    infrastructure::api_trans::collect_fee::command::{
        ProcessFeeTxCommand, ProcessFeeTxReportCommand,
    },
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
};
use dashmap::{DashMap, DashSet};
use rust_decimal::Decimal;
use std::{
    str::FromStr,
    sync::{Arc, Weak},
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool,
    entities::api_fee::{ApiFeeEntity, ApiFeeStatus},
    repositories::api_wallet::{fee::ApiFeeRepo, nonce::ApiNonceRepo},
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_types::chain::chain::ChainCode;

/// 账户级串行执行管理器
///
/// - 每个 account 对应一个 Semaphore(1)
/// - DashMap + Weak：
///   - 没有活跃任务时自动回收
///   - 不需要显式清理
/// - RAII：
///   - permit drop 即释放
///   - panic / cancel 安全
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
struct FeeTxWorkerCtx {
    ctx: &'static Context,
    core_pool: ApiWalletDbPool,
    api_transaction_pool: ApiTransactionDbPool,
    /// 同一地址的并发交易
    address_locks: Arc<AddressLockManager>,
    /// 系统级并发上限
    global_sem: Arc<Semaphore>,
    /// 同一 trade 的并发执行
    /// processing_trade is NOT a lock, only a dedup marker
    processing_trade: Arc<DashSet<String>>,
    /// 调度器自身的重入
    batch_running: Arc<Semaphore>,
    report_tx: mpsc::Sender<ProcessFeeTxReportCommand>,
}

pub(super) struct ProcessFeeTx {
    worker_ctx: FeeTxWorkerCtx,
    shutdown_rx: broadcast::Receiver<()>,
    tx_rx: mpsc::Receiver<ProcessFeeTxCommand>,
}

impl ProcessFeeTx {
    pub(super) fn new(
        ctx: &'static Context,
        core_pool: ApiWalletDbPool,
        api_transaction_pool: ApiTransactionDbPool,
        shutdown_rx: broadcast::Receiver<()>,
        tx_rx: mpsc::Receiver<ProcessFeeTxCommand>,
        report_tx: mpsc::Sender<ProcessFeeTxReportCommand>,
    ) -> Self {
        let worker_ctx = FeeTxWorkerCtx {
            ctx,
            core_pool,
            api_transaction_pool: api_transaction_pool.clone(),
            address_locks: Arc::new(AddressLockManager::new()),
            global_sem: Arc::new(Semaphore::new(32)),
            processing_trade: Arc::new(DashSet::new()),
            report_tx,
            batch_running: Arc::new(Semaphore::new(1)),
        };

        Self { worker_ctx, shutdown_rx, tx_rx }
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
                Some(cmd) = self.tx_rx.recv() => {
                    match cmd {
                        ProcessFeeTxCommand::Tx(trade_no) => {
                            self.spawn_single(&trade_no);
                            iv.reset();
                        }
                    }
                }
                _ = iv.tick() => {
                    self.spawn_batch()
                }
            }
        }
        tracing::info!("closing process fee tx ------------------------------- end");
    }

    fn spawn_single(&self, trade_no: &str) {
        let trade_no = trade_no.to_string();
        if !self.worker_ctx.processing_trade.insert(trade_no.clone()) {
            return;
        }
        let worker_ctx = self.worker_ctx.clone();

        tokio::spawn(async move {
            let _g = TradeGuard::new(&trade_no, worker_ctx.processing_trade.clone());
            tracing::info!(trade_no=%trade_no, "[手续费归集] 根据交易编号处理单个手续费交易");
            let res = ApiFeeRepo::get_api_fee_by_trade_no_status(
                &worker_ctx.api_transaction_pool,
                &trade_no,
                &[ApiFeeStatus::Init],
            )
            .await;
            match res {
                Ok(fee) => {
                    tracing::info!(trade_no=%trade_no, "[手续费归集] 找到待处理的手续费交易记录");
                    if let Err(err) = Self::process_fee_single_tx(worker_ctx, fee).await {
                        tracing::error!(trade_no=%trade_no, "[手续费归集] 处理单个手续费交易失败: {:?}", err);
                    }
                }
                Err(err) => {
                    tracing::error!(trade_no=%trade_no, "[手续费归集] 获取手续费交易记录失败: {:?}", err);
                }
            }
        });
    }

    fn spawn_batch(&self) {
        // batch 级互斥：只在这里拿一次
        let permit = match self.worker_ctx.batch_running.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                tracing::info!("[手续费归集] batch 正在运行，跳过本轮");
                return;
            }
        };

        let worker_ctx = self.worker_ctx.clone();
        tokio::spawn(async move {
            let _batch_guard = permit;
            tracing::info!("[手续费归集] 批量处理手续费交易");

            // 获取交易这里有问题
            let res = ApiFeeRepo::page_api_fee_with_status(
                &worker_ctx.api_transaction_pool,
                0,
                1000,
                &[ApiFeeStatus::Init],
            )
            .await;
            match res {
                Ok((_, transfer_fees)) => {
                    tracing::info!(
                        "[手续费归集] 找到 {} 条待处理的手续费交易记录",
                        transfer_fees.len()
                    );
                    for req in transfer_fees {
                        let worker_ctx = worker_ctx.clone();
                        let trade_no = req.trade_no.clone();
                        if !worker_ctx.processing_trade.insert(trade_no.clone()) {
                            continue;
                        }
                        let ctx2 = worker_ctx.clone();
                        tokio::spawn(async move {
                            let _g = TradeGuard::new(&trade_no, ctx2.processing_trade.clone());
                            if let Err(err) = Self::process_fee_single_tx(ctx2, req).await {
                                tracing::error!(trade_no=%trade_no, "[手续费归集] 处理单个手续费交易失败: {:?}", err);
                            }
                        });
                    }
                }
                Err(err) => {
                    tracing::error!("[手续费归集] 获取手续费交易记录列表失败: {:?}", err);
                }
            }
        });
    }

    async fn process_fee_single_tx(
        worker_ctx: FeeTxWorkerCtx,
        req: ApiFeeEntity,
    ) -> Result<(), ServiceError> {
        let from_addr = req.from_addr.clone();
        let trade_no = req.trade_no.clone();

        tracing::info!(trade_no=%trade_no, from_addr=%from_addr, "[手续费归集] 等待地址级执行权");
        let _addr_guard = worker_ctx.address_locks.acquire(&from_addr).await?;
        tracing::info!(trade_no=%trade_no, from_addr=%from_addr, "[手续费归集] 已获得地址级执行权");
        let _global_guard = worker_ctx
            .global_sem
            .acquire()
            .await
            .map_err(|_| ServiceError::System(SystemError::SemaphoreClosed))?;

        tracing::info!(trade_no=%trade_no, "[手续费归集] 处理单个手续费交易");

        // ⚠️ Step 0: 已生成raw_tx的交易优先检查链上状态
        if let Some(tx_hash) = req.tx_hash.as_deref() {
            tracing::info!(trade_no=%req.trade_no, "[手续费归集] 检测到已有raw_tx和tx_hash，执行恢复检查");

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
                    return Self::handle_fee_tx_success(worker_ctx.clone(), req, tx_resp, nonce)
                        .await;
                }
                Ok(None) => {
                    return Ok(()); // 容错，下轮再查
                }
                Err(err) => {
                    return Self::handle_fee_tx_failed(&worker_ctx, &trade_no, err).await;
                }
            }
        }

        // check
        tracing::info!(trade_no=%trade_no, "[手续费归集] 验证交易数据完整性");
        if !Self::check_digest(&worker_ctx, &req).await {
            tracing::error!(trade_no=%req.trade_no, "[手续费归集] 交易数据验证失败");
            return Self::handle_fee_tx_failed(
                &worker_ctx,
                &trade_no,
                ServiceError::Business(
                    ApiWalletError::Trans(TransError::TransactionDigestVerificationFailed).into(),
                ),
            )
            .await;
        }
        tracing::info!(trade_no=%trade_no, "[手续费归集] 交易数据验证通过");

        tracing::info!(trade_no=%trade_no, "[手续费归集] 生成转账请求");
        let transfer_req_res = Self::gen_transfer_req(&worker_ctx, &req).await;
        let result = match transfer_req_res {
            Ok(transfer_req) => {
                tracing::info!(trade_no=%trade_no, nonce=%transfer_req.nonce, "[手续费归集] 转账请求生成成功，准备发送交易");
                // 发交易
                let nonce = transfer_req.nonce;
                tracing::info!(trade_no=%trade_no, "[手续费归集] 调用转账接口发送交易");

                // 从私钥管理器获取私钥
                let from = req.from_addr.clone();
                let chain_code = req.chain_code.clone();

                tracing::info!(trade_no=%trade_no, from=%from, chain_code=%chain_code, "[手续费归集] 从私钥管理器获取私钥");
                let handles = worker_ctx.ctx.get_handles_arc().await?;
                let private_key_manager = handles.get_global_private_key_manager();
                let private_key =
                    private_key_manager.get_private_key(from.as_str(), chain_code.as_str()).await?;

                // 第一步：构建raw_tx
                let (tx_hash, raw_tx, fee) = match ApiTransDomain::build_transfer_raw(
                    transfer_req,
                    Some(private_key),
                )
                .await
                {
                    Ok((tx_hash, raw_tx, fee)) => (tx_hash, raw_tx, fee),
                    Err(err) => {
                        tracing::error!(trade_no=%trade_no, "[手续费归集] 构建raw_tx失败: {}", err);
                        return Self::handle_fee_tx_failed(
                            &worker_ctx,
                            &trade_no,
                            ServiceError::Business(
                                ApiWalletError::Trans(
                                    TransError::TransactionDigestVerificationFailed,
                                )
                                .into(),
                            ),
                        )
                        .await;
                    }
                };
                tracing::info!(trade_no=%trade_no, "[手续费归集] 构建raw_tx成功, tx_hash={}, fee={}", tx_hash, fee);

                // 第二步：将raw_tx、nonce和tx_hash落盘到数据库
                let raw_tx_str = wallet_utils::serde_func::serde_to_string(&raw_tx)?;
                let update_res = ApiFeeRepo::update_after_build(
                    &worker_ctx.api_transaction_pool,
                    &req.trade_no,
                    &tx_hash,
                    &raw_tx_str,
                    &fee,
                    nonce as i64,
                )
                .await;

                if let Err(err) = update_res {
                    tracing::error!(trade_no=%req.trade_no, "[手续费归集] 将tx_hash和raw_tx存储到数据库失败: {}", err);
                    return Self::handle_fee_tx_failed(
                        &worker_ctx,
                        &trade_no,
                        ServiceError::Database(err.into()),
                    )
                    .await;
                }
                tracing::info!(trade_no=%req.trade_no, "[手续费归集] tx_hash和raw_tx存储到数据库成功");

                // 第三步：广播交易
                let tx_resp = ApiTransDomain::broadcast_transfer(
                    &req.chain_code,
                    raw_tx,
                    Some(tx_hash.as_str()),
                )
                .await;
                match tx_resp {
                    Ok(Some(tx)) => {
                        tracing::info!(trade_no=%trade_no, tx_hash=?tx.tx_hash, "[手续费归集] 交易发送成功");
                        // 克隆worker_ctx而不是移动它，因为_global_guard仍然在借用它
                        Self::handle_fee_tx_success(worker_ctx.clone(), req, tx, nonce).await
                    }
                    Ok(None) => {
                        tracing::info!(trade_no=%trade_no, "[手续费归集] 交易广播结果不确定");
                        return Ok(());
                    }
                    Err(err) => {
                        tracing::error!(trade_no=%trade_no, "[手续费归集] 交易发送失败: {}", err);
                        // 这里使用trade_no而不是&req.trade_no
                        Self::handle_fee_tx_failed(&worker_ctx, &trade_no, err).await
                    }
                }
            }
            Err(err) => {
                tracing::error!(trade_no=%trade_no, "[手续费归集] 生成转账请求失败: {}", err);
                // 这里使用trade_no而不是&req.trade_no
                Self::handle_fee_tx_failed(&worker_ctx, &trade_no, err).await
            }
        };

        result
    }

    async fn check_digest(worker_ctx: &FeeTxWorkerCtx, req: &ApiFeeEntity) -> bool {
        tracing::info!(trade_no=%req.trade_no, "[手续费归集] 验证交易摘要");
        let sn = worker_ctx.ctx.get_sn();
        let mut d = Decimal::from_str(req.value.as_str()).unwrap();
        d = d.normalize();
        let raw_data = req.from_addr.clone() + req.to_addr.as_str() + d.to_string().as_str() + sn;
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(&raw_data));
        let is_valid = req.validate == digest;
        tracing::info!(trade_no=%req.trade_no, "[手续费归集] 摘要验证结果: {}", is_valid);
        is_valid
    }

    async fn get_eth_nonce(
        worker_ctx: &FeeTxWorkerCtx,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<i64, ServiceError> {
        tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "[手续费归集] 获取以太坊类链的nonce值");
        match ApiNonceRepo::get_api_nonce(&worker_ctx.api_transaction_pool, from_addr, chain_code)
            .await
        {
            Ok(nonce) => {
                let new_nonce = nonce + 1;
                tracing::info!(from_addr=%from_addr, chain_code=%chain_code, nonce=%new_nonce, "[手续费归集] 从数据库获取nonce并递增");
                Ok(new_nonce)
            }
            Err(_) => {
                tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "[手续费归集] 从数据库获取nonce失败，尝试从链上获取");
                let nonce = ApiTransDomain::nonce(from_addr, chain_code).await?;
                tracing::info!(from_addr=%from_addr, chain_code=%chain_code, nonce=%nonce, "[手续费归集] 从链上获取nonce成功");
                Ok(nonce as i64)
            }
        }
    }

    async fn gen_transfer_req(
        worker_ctx: &FeeTxWorkerCtx,
        req: &ApiFeeEntity,
    ) -> Result<ApiTransferReq, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, chain_code=%req.chain_code, symbol=%req.symbol, "[手续费归集] 获取代币信息");
        let coin = ApiCoinDomain::get_coin_by_token_key_exact(
            &req.chain_code,
            req.token_addr.clone().into(),
        )
        .await?;
        tracing::info!(trade_no=%req.trade_no, token_address=?coin.token_address, decimals=%coin.decimals, "[手续费归集] 代币信息获取成功");

        tracing::info!(trade_no=%req.trade_no, from_addr=%req.from_addr, to_addr=%req.to_addr, value=%req.value, "[手续费归集] 创建基础转账请求");
        let mut params =
            ApiBaseTransferReq::new(&req.from_addr, &req.to_addr, &req.value, &req.chain_code);
        let token_address = if coin.token_address.is_native() {
            None
        } else {
            let s = coin.token_address.as_db_str().to_string();
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
            ChainCode::Ethereum => {
                Self::get_eth_nonce(worker_ctx, &req.from_addr, &req.chain_code).await?
            }
            ChainCode::BnbSmartChain => {
                Self::get_eth_nonce(worker_ctx, &req.from_addr, &req.chain_code).await?
            }
            ChainCode::Litecoin => 0,
            ChainCode::Dogcoin => 0,
            ChainCode::Sui => 0,
            ChainCode::Ton => 0,
        };
        tracing::info!(trade_no=%req.trade_no, nonce=%nonce, "[手续费归集] 转账请求生成完成");
        Ok(ApiTransferReq { base: params, password: passwd, nonce: nonce as u64 })
    }

    async fn handle_fee_tx_success(
        worker_ctx: FeeTxWorkerCtx,
        req: ApiFeeEntity,
        tx: TransferResp,
        nonce: u64,
    ) -> Result<(), ServiceError> {
        tracing::info!(trade_no=%req.trade_no, tx_hash=?tx.tx_hash, "[手续费归集] 处理交易发送成功");
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
                &worker_ctx.api_transaction_pool,
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
                &worker_ctx.api_transaction_pool,
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
                worker_ctx
                    .report_tx
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
        worker_ctx: &FeeTxWorkerCtx,
        trade_no: &str,
        err: ServiceError,
    ) -> Result<(), ServiceError> {
        tracing::error!(trade_no=%trade_no, "[手续费归集] 处理交易发送失败: {}", err);
        let res = ApiFeeRepo::update_api_fee_status_and_err(
            &worker_ctx.api_transaction_pool,
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
                worker_ctx
                    .report_tx
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
