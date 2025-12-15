use crate::infrastructure::collect::command::ProcessCollectTxReportCommand;
use chrono::TimeDelta;
use dashmap::DashMap;
use serde_json::json;
use std::sync::{Arc, Weak};
use tokio::{
    sync::{Mutex, broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::collect::ApiCollectRepo,
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::transaction::{
    TransStatus, TransType, TxExecReceiptUploadReq,
};

type AddressLock = Arc<Mutex<()>>;

pub(super) struct ProcessCollectTxReport {
    pool: Arc<sqlx::SqlitePool>,
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessCollectTxReportCommand>,
    // 用于确保同一个地址的交易串行处理的互斥锁
    address_locks: DashMap<String, Weak<Mutex<()>>>,
}

impl ProcessCollectTxReport {
    pub(super) fn new(
        pool: Arc<sqlx::SqlitePool>,
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessCollectTxReportCommand>,
    ) -> Self {
        Self { pool, shutdown_rx, report_rx, address_locks: DashMap::new() }
    }

    fn get_address_lock(&self, address: &str) -> AddressLock {
        // 1. 尝试从 DashMap 中拿 Weak
        if let Some(entry) = self.address_locks.get(address) {
            if let Some(lock) = entry.value().upgrade() {
                return lock;
            }
        }

        // 2. Weak 失效 or 不存在 → 创建新的锁
        let lock = Arc::new(Mutex::new(()));

        self.address_locks.insert(address.to_string(), Arc::downgrade(&lock));

        lock
    }

    pub(super) async fn run(&mut self) {
        tracing::info!("starting process collect tx report -------------------------------");
        self.run_with_err().await;
        tracing::info!("closing process collect tx report ------------------------------- end");
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
                    tracing::info!("closing process collect tx report -------------------------------");
                    break;
                }
                report_msg = self.report_rx.recv() => {
                    if let Some(cmd) = report_msg {
                        match cmd {
                            ProcessCollectTxReportCommand::Tx(trade_no) => {
                                self.process_collect_single_tx_report_by_trade_no(&trade_no).await;
                                iv.reset();
                            }
                        }

                    }
                }
                _ = iv.tick() => {
                    self.process_collect_tx_report().await
                }
            }
        }
    }

    async fn process_collect_single_tx_report_by_trade_no(&self, trade_no: &str) {
        tracing::info!(trade_no=%trade_no, "[归集交易报告] 开始处理单个归集交易报告");
        let res = ApiCollectRepo::get_api_collect_by_trade_no_status(
            &self.pool,
            &trade_no,
            &[ApiCollectStatus::SendingTx, ApiCollectStatus::SendingTxFailed],
        )
        .await;
        match res {
            Ok(req) => {
                tracing::info!(trade_no=%trade_no, "[归集交易报告] 查询到交易信息，开始处理报告");
                let address = req.from_addr.clone();

                // 获取地址对应的锁
                let address_lock = self.get_address_lock(&address);

                // 获取锁以确保同一地址的交易串行处理
                let _guard = address_lock.lock().await;

                // 直接调用时不检查重试时间
                self.process_collect_single_tx_report(req, false).await
            }
            Err(err) => {
                tracing::warn!(trade_no=%trade_no, "[归集交易报告] 查询交易信息失败: {}", err);
            }
        }
    }

    async fn process_collect_tx_report(&self) {
        tracing::info!("[归集交易报告] 开始批量处理归集交易报告");
        let res = ApiCollectRepo::page_api_collect_with_status(
            &self.pool,
            0,
            1000,
            &[ApiCollectStatus::SendingTx, ApiCollectStatus::SendingTxFailed],
        )
        .await;
        match res {
            Ok((_, transfer_fees)) => {
                tracing::info!(
                    "[归集交易报告] 查询到 {} 笔待处理的归集交易报告",
                    transfer_fees.len()
                );

                // 并发处理不同地址的交易
                let mut tasks = vec![];
                for req in transfer_fees {
                    let address = req.from_addr.clone();
                    let address_lock = self.get_address_lock(&address);
                    let pool = self.pool.clone();

                    let task = tokio::spawn(async move {
                        // 获取地址对应的锁
                        let _guard = address_lock.lock().await;

                        // 直接处理单个交易报告
                        Self::process_single_tx_report(pool, req, true).await;
                    });

                    tasks.push(task);
                }

                // 等待所有任务完成
                for task in tasks {
                    let _ = task.await;
                }
            }
            Err(err) => {
                tracing::warn!("[归集交易报告] 批量查询交易信息失败: {}", err);
            }
        }
    }

    async fn process_collect_single_tx_report(
        &self,
        req: ApiCollectEntity,
        check_retry_time: bool,
    ) {
        Self::process_single_tx_report(self.pool.clone(), req, check_retry_time).await;
    }

    /// 静态方法：处理单个交易报告
    async fn process_single_tx_report(
        pool: Arc<sqlx::SqlitePool>,
        req: ApiCollectEntity,
        check_retry_time: bool,
    ) {
        tracing::info!(trade_no=%req.trade_no, status=%req.status, "[归集交易报告] 开始处理单条归集交易报告");

        // 只有在需要检查重试时间时才执行检查
        if check_retry_time {
            // 判断超时时间
            let now = chrono::Utc::now();
            let timeout = now - req.updated_at.unwrap();
            tracing::info!(trade_no=%req.trade_no, "[归集交易报告] 当前时间: {}, 上次更新时间: {}, 超时时间: {}, 当前重试次数: {}", 
                        now, req.updated_at.unwrap(), timeout, req.post_tx_count);

            if timeout < TimeDelta::seconds(1 << req.post_tx_count as i64) {
                tracing::warn!(trade_no=%req.trade_no, "[归集交易报告] 未到重试时间，跳过本次处理");
                return;
            }
        } else {
            tracing::info!(trade_no=%req.trade_no, "[归集交易报告] 直接调用，跳过重试时间检查");
        }

        let (status, remark) = if req.status == ApiCollectStatus::SendingTxFailed {
            let msg = json!({
                "code": req.err_code,
                "msg": req.err_msg,
            });
            let s = msg.to_string();
            tracing::info!(trade_no=%req.trade_no, "[归集交易报告] 交易发送失败，准备上传失败报告: {}", s);
            (TransStatus::Fail, s)
        } else {
            tracing::info!(trade_no=%req.trade_no, "[归集交易报告] 交易发送成功，准备上传成功报告");
            (TransStatus::Success, "".to_string())
        };

        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        tracing::info!(trade_no=%req.trade_no, "[归集交易报告] 准备调用后端API上传执行结果");

        match backend_api
            .upload_tx_exec_receipt(&TxExecReceiptUploadReq::new(
                &req.trade_no,
                TransType::Col,
                &req.tx_hash,
                status,
                remark.as_str(),
            ))
            .await
        {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[归集交易报告] 上传执行结果成功");
                Self::handle_report_success(pool.clone(), req).await
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "[归集交易报告] 上传执行结果失败: {}", err);
                Self::handle_report_failed(pool.clone(), req, err).await
            }
        }
    }

    async fn handle_report_success(pool: Arc<sqlx::SqlitePool>, req: ApiCollectEntity) {
        let (next_status, notes) = if req.status == ApiCollectStatus::SendingTxFailed {
            tracing::info!(trade_no=%req.trade_no, "[归集交易报告] 交易失败报告上传成功，准备更新状态为SendingTxFailedReport");
            (ApiCollectStatus::SendingTxFailedReport, "uploaded server ok for collect tx failed")
        } else {
            tracing::info!(trade_no=%req.trade_no, "[归集交易报告] 交易成功报告上传成功，准备更新状态为SendingTxReport");
            (ApiCollectStatus::SendingTxReport, "uploaded server ok for collect tx success")
        };

        let res = ApiCollectRepo::update_api_collect_next_status(
            &pool,
            &req.trade_no,
            req.status,
            next_status,
        )
        .await;

        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[归集交易报告] 更新交易状态成功，新状态: {}", next_status);
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[归集交易报告] 更新交易状态失败: {}", err);
            }
        }
    }

    async fn handle_report_failed(
        pool: Arc<sqlx::SqlitePool>,
        req: ApiCollectEntity,
        err: wallet_transport_backend::Error,
    ) {
        tracing::warn!(trade_no=%req.trade_no, "[归集交易报告] 上传报告失败，准备增加重试次数: {}", err);
        let res =
            ApiCollectRepo::update_api_collect_post_tx_count(&pool, &req.trade_no, req.status)
                .await;

        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[归集交易报告] 增加重试次数成功，当前重试次数: {}", req.post_tx_count + 1);
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[归集交易报告] 更新重试次数失败: {}", err);
            }
        }
    }
}
