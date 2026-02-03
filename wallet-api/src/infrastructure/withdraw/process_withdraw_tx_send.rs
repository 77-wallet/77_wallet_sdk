// process_withdraw_tx_send.rs
//! # 后台任务并发模型说明（请务必阅读）
//!
//! 本模块负责【提币交易发送】相关的后台任务处理，
//! 其并发模型是 **刻意设计过的**，不是临时写法。
//!
//! ------------------------------------------------------------
//! ## 一、我们在解决什么问题？
//!
//! 后台任务系统需要同时满足：
//!
//! 1. **高并发吞吐**
//!    - 不同地址（from_addr）的任务应当并行执行
//!
//! 2. **地址级严格串行**
//!    - 同一个地址的任务 **绝不能并发**
//!    - 否则会导致：nonce 错乱 / 状态覆盖 / 重复上报
//!
//! 3. **可控的生命周期**
//!    - 不活跃地址不应永久占用内存
//!    - 系统不能维护一个"永不释放的锁表"
//!
//! ------------------------------------------------------------
//! ## 二、当前并发模型（非常重要）
//!
//! ### 并发粒度原则
//!
//! > **并发的最小粒度是「地址」**
//!
//! - 地址之间：并行
//! - 同一地址内：严格串行
//!
//! ------------------------------------------------------------
//! ## 三、实现方式
//!
//! 当前采用：
//!
//! - `DashMap<Address, Weak<Semaphore>>`
//! - `Semaphore(1)` 作为地址级串行保证
//!
//! 各组件职责：
//!
//! - **DashMap**
//!   - 并发安全地管理「地址 → 锁」的映射
//!
//! - **Semaphore(1)**
//!   - 保证同一地址一次只执行一个任务
//!
//! - **Weak**
//!   - 当某个地址不再有任务执行时，
//!     对应的锁会被自动回收，避免内存泄漏
//!
//! ⚠️ 注意：
//! - 锁只用于【并发控制】
//! - **绝不保存任何业务状态**
//!
//! ------------------------------------------------------------
//! ## 四、为什么不用全局 Mutex？（禁止）
//!
//! ```ignore
//! static GLOBAL_LOCK: Mutex<()> = Mutex::new(());
//! ```
//!
//! 原因：
//! - 会把所有地址串行化
//! - 一个慢地址会拖垮整个系统
//! - 吞吐量无法随地址数扩展
//!
//! 👉 这是明确禁止的实现方式
//!
//! ------------------------------------------------------------
//! ## 五、业务代码必须遵守的规则
//!
//! ### ✅ 允许
//!
//! - 只通过统一逻辑获取地址级执行权
//! - 在获得 permit 后执行业务逻辑
//! - 任务结束后自然释放（RAII）
//!
//! ```rust,ignore
//! let _permit = address_lock.acquire().await;
//! do_work().await;
//! ```
//!
//! ------------------------------------------------------------
//! ### ❌ 严格禁止（非常重要）
//!
//! #### 1️⃣ 不要在业务代码中私自创建锁
//!
//! ```rust,ignore
//! // ❌ 禁止
//! let lock = Mutex::new(());
//! ```
//!
//! 否则会破坏地址级串行保证，
//! 产生极难排查的并发 Bug。
//!
//! ------------------------------------------------------------
//! #### 2️⃣ 不要在锁内跨 await 持有其他 Mutex
//!
//! ```rust,ignore
//! // ❌ 禁止
//! let _permit = address_lock.acquire().await;
//! let data = mutex.lock().await; // 死锁风险！
//! do_async_work().await;
//! ```
//!
//! 正确做法：
//! ```rust,ignore
//! // ✅ 正确
//! let _permit = address_lock.acquire().await;
//! let data = mutex.lock().await;
//! drop(data); // 手动释放
//! do_async_work().await;
//! ```
//!
//! ------------------------------------------------------------
//! #### 3️⃣ 不要在锁内进行长时间阻塞操作
//!
//! ```rust,ignore
//! // ❌ 禁止
//! let _permit = address_lock.acquire().await;
//! std::thread::sleep(Duration::from_secs(10)); // 占用锁时间过长
//! ```
//!
//! ------------------------------------------------------------

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
    infrastructure::withdraw::command::{ProcessWithdrawTxCommand, ProcessWithdrawTxReportCommand},
    messaging::notify::{FrontendNotifyEvent, api_wallet::WithdrawFront, event::NotifyEvent},
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
    CollectDbPool, CoreDbPool,
    entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus, ErrCode},
    repositories::api_wallet::{nonce::ApiNonceRepo, withdraw::ApiWithdrawRepo},
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_types::chain::chain::ChainCode;

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
struct WithdrawTxWorkerCtx {
    core_pool: CoreDbPool,
    api_fund_pool: CollectDbPool,
    address_locks: Arc<AddressLockManager>,
    global_sem: Arc<Semaphore>,
    processing_trade: Arc<DashSet<String>>,
    batch_running: Arc<Semaphore>,
    report_tx: mpsc::Sender<ProcessWithdrawTxReportCommand>,
    ctx: &'static Context,
}

struct TradeGuard {
    trade_no: String,
    processing_trade: Arc<DashSet<String>>,
}

impl TradeGuard {
    fn new(trade_no: &str, processing_trade: Arc<DashSet<String>>) -> Self {
        Self { trade_no: trade_no.to_string(), processing_trade }
    }
}

impl Drop for TradeGuard {
    fn drop(&mut self) {
        self.processing_trade.remove(&self.trade_no);
    }
}

pub(super) struct ProcessWithdrawTx {
    worker_ctx: WithdrawTxWorkerCtx,
    shutdown_rx: broadcast::Receiver<()>,
    tx_rx: mpsc::Receiver<ProcessWithdrawTxCommand>,
}

impl ProcessWithdrawTx {
    pub(super) fn new(
        ctx: &'static Context,
        core_pool: CoreDbPool,
        pool: CollectDbPool,
        shutdown_rx: broadcast::Receiver<()>,
        tx_rx: mpsc::Receiver<ProcessWithdrawTxCommand>,
        report_tx: mpsc::Sender<ProcessWithdrawTxReportCommand>,
    ) -> Self {
        let worker_ctx = WithdrawTxWorkerCtx {
            core_pool: core_pool.clone(),
            api_fund_pool: pool.clone(),
            address_locks: Arc::new(AddressLockManager::new()),
            global_sem: Arc::new(Semaphore::new(32)), // 与 collect 模块保持一致
            processing_trade: Arc::new(DashSet::new()),
            report_tx: report_tx.clone(),
            batch_running: Arc::new(Semaphore::new(1)),
            ctx,
        };

        Self { shutdown_rx, tx_rx, worker_ctx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!("starting process withdraw -------------------------------");
        self.run_with_err().await;
        tracing::info!("closing process withdraw tx ------------------------------- end");
    }

    async fn run_with_err(&mut self) {
        tracing::info!("withdraw_tx:send: 启动提币交易处理循环");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                tracing::warn!("withdraw_tx:send: 共享密钥未设置，等待10秒后重试");
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("withdraw_tx:send: 接收到关闭信号，退出处理循环");
                    break;
                }
                msg = self.tx_rx.recv() => {
                    if let Some(cmd) = msg {
                        match cmd {
                            ProcessWithdrawTxCommand::Tx(trade_no) => {
                                tracing::info!(trade_no=%trade_no, "withdraw_tx:send: 接收到单个交易处理请求");
                                self.spawn_single(&trade_no);
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    tracing::info!("withdraw_tx:send: 执行定时批量处理提币交易");
                    self.spawn_batch()
                }
            }
        }
    }

    fn spawn_single(&self, trade_no: &str) {
        let ctx = self.worker_ctx.clone();
        let trade_no = trade_no.to_string();

        tokio::spawn(async move {
            let req = match ApiWithdrawRepo::get_api_withdraw_by_trade_no_status(
                &ctx.api_fund_pool,
                &trade_no,
                &[ApiWithdrawStatus::AuditPass],
            )
            .await
            {
                Ok(res) => res,
                Err(err) => {
                    tracing::warn!(trade_no=%trade_no, "process withdraw tx not found: {}", err);
                    return;
                }
            };
            if !ctx.processing_trade.insert(req.trade_no.clone()) {
                tracing::warn!(trade_no=%req.trade_no, "withdraw tx already processing, skip");
                return;
            }
            let _guard = TradeGuard::new(&req.trade_no, ctx.processing_trade.clone());

            if let Err(e) = Self::process_withdraw_single_tx(ctx, req).await {
                tracing::error!(trade_no=%trade_no, "withdraw_tx:send: 处理单个提币交易失败: {}", e);
            }
        });
    }

    fn spawn_batch(&self) {
        // batch 级互斥：只在这里拿一次
        let permit = match self.worker_ctx.batch_running.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                tracing::info!("withdraw_tx:send: batch 正在运行，跳过本轮");
                return;
            }
        };

        tracing::info!("withdraw_tx:send: 查询待处理的提币交易");
        let ctx = self.worker_ctx.clone();

        tokio::spawn(async move {
            let _batch_guard = permit;
            let res = ApiWithdrawRepo::list_api_withdraw_with_status(
                &ctx.api_fund_pool,
                vec![ApiWithdrawStatus::AuditPass],
                0,
                1000,
            )
            .await;
            let withdraw_txs = match res {
                Ok(v) => v,
                Err(err) => {
                    tracing::warn!("process_withdraw_tx_send 查询待处理提币交易失败: {}", err);
                    return;
                }
            };
            tracing::info!("withdraw_tx:send: 找到 {} 笔待处理的提币交易", withdraw_txs.len());
            for req in withdraw_txs {
                let ctx = ctx.clone();
                let trade_no = req.trade_no.clone(); // 提前克隆trade_no
                if !ctx.processing_trade.insert(trade_no.clone()) {
                    continue;
                }
                tokio::spawn(async move {
                    let _guard = TradeGuard::new(&trade_no, ctx.processing_trade.clone());
                    if let Err(err) = Self::process_withdraw_single_tx(ctx, req).await {
                        tracing::error!(trade_no=%trade_no, "withdraw_tx:send: 处理单个提币交易失败: {}", err);
                    }
                });
            }
        });
    }

    async fn process_withdraw_single_tx(
        worker_ctx: WithdrawTxWorkerCtx,
        req: ApiWithdrawEntity,
    ) -> Result<(), ServiceError> {
        let _addr_guard = worker_ctx.address_locks.acquire(&req.from_addr).await?;
        let _global_guard = worker_ctx
            .global_sem
            .acquire()
            .await
            .map_err(|_| ServiceError::System(SystemError::SemaphoreClosed))?;

        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 开始处理提币交易, from={}, to={}, value={}, chain={}, symbol={}", 
            req.from_addr, req.to_addr, req.value, req.chain_code, req.symbol);

        // ⚠️ Step 0: 已生成raw_tx的交易优先检查链上状态
        if let Some(tx_hash) = req.tx_hash.as_deref() {
            tracing::info!(trade_no=%req.trade_no, "withdraw_tx: 检测到已有raw_tx和tx_hash，执行恢复检查");

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
                    return Self::handle_withdraw_tx_success(&worker_ctx, req, tx_resp, nonce)
                        .await;
                }
                Ok(None) => {
                    return Ok(()); // 容错，下轮再查
                }
                Err(err) => {
                    return Self::handle_withdraw_tx_failed(
                        &worker_ctx,
                        &req,
                        err,
                        ErrCode::UnknownError,
                    )
                    .await;
                }
            }
        }

        // 检查交易摘要
        if !Self::check_digest(&req).await {
            tracing::error!(trade_no=%req.trade_no, "withdraw_tx:send: 交易摘要验证失败");
            return Self::handle_withdraw_tx_failed(
                &worker_ctx,
                &req,
                ServiceError::Business(
                    ApiWalletError::Trans(TransError::TransactionDigestVerificationFailed).into(),
                ),
                ErrCode::UnknownError,
            )
            .await;
        }
        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 交易摘要验证通过");

        // 生成转账请求
        let transfer_req_res = Self::gen_transfer_req(&worker_ctx, &req).await;
        match transfer_req_res {
            Ok(transfer_req) => {
                tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 生成转账请求成功，准备发送交易");

                // 发送交易
                let nonce = transfer_req.nonce;
                tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 开始发送提币交易, nonce={}", nonce);

                // 第一步：构建raw_tx
                let (tx_hash, raw_tx, fee) = match ApiTransDomain::build_transfer_raw(
                    transfer_req,
                    None,
                )
                .await
                {
                    Ok((tx_hash, raw_tx, fee)) => (tx_hash, raw_tx, fee),
                    Err(err) => {
                        tracing::error!(trade_no=%req.trade_no, "withdraw_tx:send: 构建raw_tx失败: {}", err);
                        return Self::handle_withdraw_tx_failed(
                            &worker_ctx,
                            &req,
                            ServiceError::Business(
                                ApiWalletError::Trans(TransError::BuildWithdrawTransactionFailed(
                                    err.to_string(),
                                ))
                                .into(),
                            ),
                            ErrCode::UnknownError,
                        )
                        .await;
                    }
                };
                tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 构建raw_tx成功, tx_hash={}, fee={}", tx_hash, fee);

                // 第二步：将raw_tx、nonce和tx_hash落盘到数据库
                let raw_tx_str = wallet_utils::serde_func::serde_to_string(&raw_tx)?;
                let update_res = ApiWithdrawRepo::update_after_build(
                    &worker_ctx.api_fund_pool,
                    &req.trade_no,
                    &tx_hash,
                    &raw_tx_str,
                    &fee,
                    nonce as i64,
                )
                .await;

                if let Err(err) = update_res {
                    tracing::error!(trade_no=%req.trade_no, "withdraw_tx:send: 将tx_hash和raw_tx存储到数据库失败: {}", err);
                    return Self::handle_withdraw_tx_failed(
                        &worker_ctx,
                        &req,
                        ServiceError::Database(err.into()),
                        ErrCode::SDKInternalError,
                    )
                    .await;
                }
                tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: tx_hash和raw_tx存储到数据库成功");

                // 第三步：广播交易
                let tx_resp = ApiTransDomain::broadcast_transfer(&req.chain_code, raw_tx).await;
                match tx_resp {
                    Ok(Some(tx)) => {
                        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 发送交易成功, tx_hash={}", tx.tx_hash);
                        return Self::handle_withdraw_tx_success(&worker_ctx, req, tx, nonce).await;
                    }
                    Ok(None) => {
                        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 交易广播结果不确定");
                        return Ok(());
                    }
                    Err(err) => {
                        tracing::error!(trade_no=%req.trade_no, "withdraw_tx:send: 发送交易失败: {}", err);
                        // 检查是否为超时错误
                        let err_str = err.to_string();
                        let err_code = if err_str.contains("operation timed out")
                            || err_str.contains("is_timeout: true")
                        {
                            tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 超时错误，使用错误码6006");
                            ErrCode::TransactionOnChainException
                        } else {
                            tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 非超时错误，使用默认错误码6099");
                            ErrCode::UnknownError
                        };
                        return Self::handle_withdraw_tx_failed(&worker_ctx, &req, err, err_code)
                            .await;
                    }
                }
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "withdraw_tx:send: 生成转账请求失败: {}", err);
                return Self::handle_withdraw_tx_failed(
                    &worker_ctx,
                    &req,
                    err,
                    ErrCode::UnknownError,
                )
                .await;
            }
        }
    }

    async fn check_digest(req: &ApiWithdrawEntity) -> bool {
        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 开始验证交易摘要");
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let mut d = Decimal::from_str(req.value.as_str()).unwrap();
        d = d.normalize();
        let raw_data = req.from_addr.clone() + req.to_addr.as_str() + d.to_string().as_str() + sn;
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(&raw_data));

        let is_valid = req.validate == digest;
        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 交易摘要验证完成, 结果: {}", is_valid);
        is_valid
    }

    async fn get_eth_nonce(
        pool: &CollectDbPool,
        from_addr: &str,
        chain_code: &str,
    ) -> Result<i64, ServiceError> {
        tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "withdraw_tx:send: 获取以太坊nonce");
        match ApiNonceRepo::get_api_nonce(&pool, from_addr, chain_code).await {
            Ok(nonce) => {
                let next_nonce = nonce + 1;
                tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "withdraw_tx:send: 从本地缓存获取nonce: {}, 下一个nonce: {}", nonce, next_nonce);
                Ok(next_nonce)
            }
            Err(_) => {
                tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "withdraw_tx:send: 本地缓存未找到nonce，从链上获取");
                let nonce = ApiTransDomain::nonce(from_addr, chain_code).await?;
                tracing::info!(from_addr=%from_addr, chain_code=%chain_code, "withdraw_tx:send: 从链上获取nonce: {}", nonce);
                Ok(nonce as i64)
            }
        }
    }

    async fn gen_transfer_req(
        worker_ctx: &WithdrawTxWorkerCtx,
        req: &ApiWithdrawEntity,
    ) -> Result<ApiTransferReq, ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 开始生成转账请求");

        // 获取币种信息
        let coin =
            ApiCoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_addr.clone()).await?;
        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 获取币种信息成功, symbol={}, token_address={:?}, decimals={}", 
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
        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 创建基础转账请求成功");

        // 获取钱包密码
        let passwd = ApiWalletDomain::get_passwd().await?;
        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 获取钱包密码成功");

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
        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 计算nonce成功, nonce={}", nonce);

        let transfer_req = ApiTransferReq { base: params, password: passwd, nonce: nonce as u64 };
        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 生成转账请求成功");
        Ok(transfer_req)
    }

    async fn handle_withdraw_tx_success(
        worker_ctx: &WithdrawTxWorkerCtx,
        req: ApiWithdrawEntity,
        tx: TransferResp,
        nonce: u64,
    ) -> Result<(), ServiceError> {
        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 处理交易成功结果");

        // 发送前端通知
        let data = NotifyEvent::Withdraw(WithdrawFront {
            uid: req.uid.to_string(),
            from_addr: req.from_addr.to_string(),
            to_addr: req.to_addr.to_string(),
            value: req.value.to_string(),
        });
        _ = FrontendNotifyEvent::new(data).send().await;

        let resource_consume = tx.resource_consume().unwrap_or_else(|_| "".to_string());
        tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 交易资源消耗: {}, 手续费: {}", resource_consume, tx.fee);

        // 更新交易状态
        let res = if req.chain_code == ChainCode::Ethereum.to_string()
            || req.chain_code == ChainCode::BnbSmartChain.to_string()
        {
            tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 更新以太坊/BSC交易状态，包含nonce");
            ApiWithdrawRepo::update_api_withdraw_tx_status_nonce(
                &worker_ctx.api_fund_pool,
                &req.from_addr,
                &req.chain_code,
                &req.trade_no,
                nonce as i64,
                &tx.tx_hash,
                &resource_consume,
                &tx.fee,
                ApiWithdrawStatus::SendingTx,
            )
            .await
        } else {
            tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 更新非以太坊/BSC交易状态");
            ApiWithdrawRepo::update_api_withdraw_tx_status(
                &worker_ctx.api_fund_pool,
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
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 更新交易状态成功，交易已发送");
                // 上报交易不影响交易偏移量计算
                tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 准备上报交易结果");
                worker_ctx
                    .report_tx
                    .send(ProcessWithdrawTxReportCommand::Tx(req.trade_no.to_string()))
                    .await
                    .map_err(|e| {
                        ServiceError::System(SystemError::ChannelSendFailed(e.to_string()))
                    })?;
                tracing::info!(trade_no=%req.trade_no, "withdraw_tx:send: 交易上报完成");
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "withdraw_tx:send: 更新交易状态失败: {}", err);
            }
        }
        Ok(())
    }

    async fn handle_withdraw_tx_failed(
        worker_ctx: &WithdrawTxWorkerCtx,
        req: &ApiWithdrawEntity,
        err: ServiceError,
        err_code: ErrCode,
    ) -> Result<(), ServiceError> {
        let trade_no = req.trade_no.to_string();
        tracing::info!(trade_no=%trade_no, "withdraw_tx:send: 处理交易失败结果, 错误: {}, 错误码: {}", err, err_code);

        // 发送前端通知
        let data = NotifyEvent::Withdraw(WithdrawFront {
            uid: req.uid.to_string(),
            from_addr: req.from_addr.to_string(),
            to_addr: req.to_addr.to_string(),
            value: req.value.to_string(),
        });
        _ = FrontendNotifyEvent::new(data).send().await;
        // 更新交易状态,发送失败
        let res = ApiWithdrawRepo::update_api_withdraw_status_and_err(
            &worker_ctx.api_fund_pool,
            &trade_no,
            ApiWithdrawStatus::SendingTxFailed,
            err_code,
            &err.to_string(),
        )
        .await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%trade_no, "withdraw_tx:send: 更新交易状态为失败成功");
                // 上报交易不影响交易偏移量计算
                tracing::info!(trade_no=%trade_no, "withdraw_tx:send: 准备上报失败交易");
                worker_ctx
                    .report_tx
                    .send(ProcessWithdrawTxReportCommand::Tx(trade_no.to_string()))
                    .await
                    .map_err(|e| {
                        ServiceError::System(SystemError::ChannelSendFailed(e.to_string()))
                    })?;
                tracing::info!(trade_no=%trade_no, "withdraw_tx:send: 失败交易上报完成");
            }
            Err(update_err) => {
                tracing::error!(trade_no=%trade_no, "withdraw_tx:send: 更新交易状态为失败失败: {}", update_err);
            }
        }
        Ok(())
    }
}
