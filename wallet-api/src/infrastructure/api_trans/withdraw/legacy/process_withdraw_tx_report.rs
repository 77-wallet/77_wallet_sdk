// legacy withdraw transaction report worker.
// process_withdraw_tx_report.rs
#![allow(deprecated)]

use crate::infrastructure::api_trans::withdraw::command::ProcessWithdrawTxReportCommand;
use chrono::TimeDelta;
use dashmap::DashMap;
use serde_json::json;
use std::sync::{Arc, Weak};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    ApiTransactionDbPool,
    entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
    repositories::api_wallet::withdraw::ApiWithdrawRepo,
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::transaction::{
    TransStatus, TransType, TxExecReceiptUploadReq,
};

/// 账户级串行执行管理器
///
/// - 每个 account 对应一个 Semaphore(1)
/// - DashMap + Weak：
///   - 没有活跃任务时自动回收
///   - 不需要显式清理
/// - RAII：
///   - permit drop 即释放
///   - panic / cancel 安全
#[derive(Clone)]
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
    pub async fn acquire(
        &self,
        account: &str,
    ) -> Result<OwnedSemaphorePermit, crate::error::service::ServiceError> {
        let sem = self.get_or_create_semaphore(account);
        sem.acquire_owned().await.map_err(|_| {
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal("SemaphoreClosed".to_string()),
            )
        })
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

/// 凡是"上报 / 通知 / 回执"类模块：
/// 不要 batch_running
/// 不要 processing_set
/// 只要 address lock + global semaphore
#[derive(Clone)]
struct WithdrawTxWorkerCtx {
    pool: ApiTransactionDbPool,
    address_locks: AddressLockManager,
    global_sem: Arc<Semaphore>,
    backend_api: Arc<wallet_transport_backend::api::BackendApi>,
}

impl WithdrawTxWorkerCtx {
    async fn get_address_lock(
        &self,
        address: &str,
    ) -> Result<OwnedSemaphorePermit, crate::error::service::ServiceError> {
        self.address_locks.acquire(address).await
    }
}

pub(super) struct ProcessWithdrawTxReport {
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessWithdrawTxReportCommand>,
    worker_ctx: WithdrawTxWorkerCtx,
}

impl ProcessWithdrawTxReport {
    pub(super) fn new(
        ctx: &'static crate::context::Context,
        pool: ApiTransactionDbPool,
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessWithdrawTxReportCommand>,
    ) -> Self {
        let worker_ctx = WithdrawTxWorkerCtx {
            pool: pool.clone(),
            address_locks: AddressLockManager::new(),
            global_sem: Arc::new(Semaphore::new(64)),
            backend_api: ctx.get_global_backend_api(),
        };

        Self { shutdown_rx, report_rx, worker_ctx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!("starting process withdraw tx report -------------------------------");
        self.run_with_err().await;
        tracing::info!("closing process withdraw tx report ------------------------------- end");
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
                    tracing::info!("closing process withdraw tx report -------------------------------");
                    break;
                }
                report_msg = self.report_rx.recv() => {
                    if let Some(cmd) = report_msg {
                        match cmd {
                            ProcessWithdrawTxReportCommand::Tx(trade_no) => {
                                self.spawn_single(&trade_no);
                                iv.reset();
                            }
                        }

                    }
                }
                _ = iv.tick() => {
                    self.spawn_batch();
                }
            }
        }
    }

    fn spawn_single(&self, trade_no: &str) {
        let ctx = self.worker_ctx.clone();
        let trade_no = trade_no.to_string();
        tracing::info!(trade_no=%trade_no, "[提币交易报告] 开始处理单个提币交易报告");
        tokio::spawn(async move {
            let req = match ApiWithdrawRepo::get_api_withdraw_by_trade_no_status(
                &ctx.pool,
                &trade_no,
                &[ApiWithdrawStatus::SendingTx, ApiWithdrawStatus::SendingTxFailed],
            )
            .await
            {
                Ok(req) => req,
                Err(err) => {
                    tracing::warn!(trade_no=%trade_no, "[提币交易报告] 查询交易信息失败: {}", err);
                    return;
                }
            };
            tracing::info!(trade_no=%trade_no, "[提币交易报告] 查询到交易信息，开始处理报告");
            let _permit = match ctx.get_address_lock(&req.from_addr).await {
                Ok(permit) => permit,
                Err(err) => {
                    tracing::warn!(trade_no=%trade_no, "[提币交易报告] 获取地址锁失败: {}", err);
                    return;
                }
            };
            let _global_permit = ctx.global_sem.acquire().await.unwrap();

            // 直接调用时不检查重试时间
            Self::process_single_tx_report(ctx.pool, ctx.backend_api.clone(), req, false).await
        });
    }

    fn spawn_batch(&self) {
        let ctx = self.worker_ctx.clone();
        tracing::info!("[提币交易报告] 开始批量处理提币交易报告");

        tokio::spawn(async move {
            let res = ApiWithdrawRepo::list_api_withdraw_with_status(
                &ctx.pool,
                vec![ApiWithdrawStatus::SendingTx, ApiWithdrawStatus::SendingTxFailed],
                0,
                1000,
            )
            .await;
            let withdraws = match res {
                Ok(v) => v,
                Err(err) => {
                    tracing::warn!("[提币交易报告] 批量查询交易信息失败: {}", err);
                    return;
                }
            };
            tracing::info!("[提币交易报告] 查询到 {} 笔待处理的提币交易报告", withdraws.len());

            for req in withdraws {
                let ctx = ctx.clone();

                tokio::spawn(async move {
                    let _permit = match ctx.get_address_lock(&req.from_addr).await {
                        Ok(permit) => permit,
                        Err(err) => {
                            tracing::warn!(trade_no=%req.trade_no, "[提币交易报告] 获取地址锁失败: {}", err);
                            return;
                        }
                    };
                    let _global_permit = ctx.global_sem.acquire().await.unwrap();

                    Self::process_single_tx_report(
                        ctx.pool.clone(),
                        ctx.backend_api.clone(),
                        req,
                        true,
                    )
                    .await
                });
            }
        });
    }

    /// 静态方法：处理单个交易报告
    async fn process_single_tx_report(
        pool: ApiTransactionDbPool,
        backend_api: Arc<wallet_transport_backend::api::BackendApi>,
        req: ApiWithdrawEntity,
        check_retry_time: bool,
    ) {
        tracing::info!(trade_no=%req.trade_no, status=%req.status, "[提币交易报告] 开始处理单条提币交易报告");

        // 只有在需要检查重试时间时才执行检查
        if check_retry_time {
            // 判断超时时间
            let now = chrono::Utc::now();
            let timeout = now - req.updated_at.unwrap();
            let max_backoff = 60; // 60秒
            let backoff = (1i64 << req.post_tx_count).clamp(1, max_backoff);
            tracing::info!(trade_no=%req.trade_no, "[提币交易报告] 当前时间: {}, 上次更新时间: {}, 超时时间: {}, 当前重试次数: {}, 退避时间: {}秒", 
                        now, req.updated_at.unwrap(), timeout, req.post_tx_count, backoff);

            if timeout < TimeDelta::seconds(backoff) {
                tracing::warn!(trade_no=%req.trade_no, "[提币交易报告] 未到重试时间，跳过本次处理");
                return;
            }
        } else {
            tracing::info!(trade_no=%req.trade_no, "[提币交易报告] 直接调用，跳过重试时间检查");
        }

        let (status, remark, error_code) = if req.status == ApiWithdrawStatus::SendingTxFailed {
            if let Some(err_code) = req.err_code {
                let msg = json!({
                    "code": format!("ERR_{}", err_code),
                    "msg": req.err_msg,
                });
                let s = msg.to_string();
                tracing::info!(trade_no=%req.trade_no, "[提币交易报告] 交易发送失败，准备上传失败报告: {}", s);
                (TransStatus::Fail, s, Some(err_code.to_string()))
            } else {
                (TransStatus::Success, "".to_string(), None)
            }
        } else {
            tracing::info!(trade_no=%req.trade_no, "[提币交易报告] 交易发送成功，准备上传成功报告");
            (TransStatus::Success, "".to_string(), None)
        };

        tracing::info!(trade_no=%req.trade_no, "[提币交易报告] 准备调用后端API上传执行结果");

        // 创建请求对象
        let mut tx_req = TxExecReceiptUploadReq::new(
            None,
            None,
            &req.trade_no,
            TransType::Wd,
            req.tx_hash.as_deref(),
            status,
            remark.as_str(),
        );

        // 如果有错误码，添加到请求中
        if let Some(code) = error_code {
            tx_req = tx_req.with_error_code(&code);
        }

        match backend_api.upload_tx_exec_receipt(&tx_req).await {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[提币交易报告] 上传执行结果成功");
                Self::handle_report_success(pool.clone(), req).await
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "[提币交易报告] 上传执行结果失败: {}", err);
                Self::handle_report_failed(pool.clone(), req, err).await
            }
        }
    }

    async fn handle_report_success(pool: ApiTransactionDbPool, req: ApiWithdrawEntity) {
        let (next_status, _notes) = if req.status == ApiWithdrawStatus::SendingTxFailed {
            tracing::info!(trade_no=%req.trade_no, "[提币交易报告] 交易失败报告上传成功，准备更新状态为SendingTxFailedReport");
            (ApiWithdrawStatus::SendingTxFailedReport, "uploaded server ok for withdraw tx failed")
        } else {
            tracing::info!(trade_no=%req.trade_no, "[提币交易报告] 交易成功报告上传成功，准备更新状态为SendingTxReport");
            (ApiWithdrawStatus::SendingTxReport, "uploaded server ok for withdraw tx success")
        };

        let res = ApiWithdrawRepo::update_api_withdraw_next_status(
            &pool,
            &req.trade_no,
            req.status,
            next_status,
        )
        .await;

        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[提币交易报告] 更新交易状态成功，新状态: {}", next_status);
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[提币交易报告] 更新交易状态失败: {}", err);
            }
        }
    }

    async fn handle_report_failed(
        pool: ApiTransactionDbPool,
        req: ApiWithdrawEntity,
        err: wallet_transport_backend::Error,
    ) {
        tracing::warn!(trade_no=%req.trade_no, "[提币交易报告] 上传报告失败，准备增加重试次数: {}", err);
        let res =
            ApiWithdrawRepo::update_api_fee_post_tx_count(&pool, &req.trade_no, req.status).await;

        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[提币交易报告] 增加重试次数成功，当前重试次数: {}", req.post_tx_count + 1);
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[提币交易报告] 更新重试次数失败: {}", err);
            }
        }
    }
}
