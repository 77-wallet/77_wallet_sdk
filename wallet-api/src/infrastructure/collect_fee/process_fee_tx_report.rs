use crate::infrastructure::collect_fee::command::ProcessFeeTxReportCommand;
use chrono::TimeDelta;
use dashmap::DashMap;
use serde_json::json;
use std::sync::{Arc, Weak};
use tokio::{
    sync::{Mutex, Semaphore, broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    entities::api_fee::{ApiFeeEntity, ApiFeeStatus},
    repositories::api_wallet::fee::ApiFeeRepo,
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::transaction::{
    TransStatus, TransType, TxExecReceiptUploadReq,
};

#[derive(Clone)]
struct FeeTxWorkerCtx {
    pool: Arc<sqlx::SqlitePool>,
    address_locks: Arc<DashMap<String, Weak<Mutex<()>>>>,
    global_sem: Arc<Semaphore>,
}

impl FeeTxWorkerCtx {
    /// 获取地址对应的锁
    fn get_address_lock(&self, address: &str) -> Arc<Mutex<()>> {
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
}

pub(super) struct ProcessFeeTxReport {
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessFeeTxReportCommand>,
    // // address 级串行
    // address_locks: DashMap<String, Weak<Mutex<()>>>,
    // // 全局并发限制（关键）
    // global_sem: Arc<Semaphore>,
    worker_ctx: FeeTxWorkerCtx,
}

impl ProcessFeeTxReport {
    pub(super) fn new(
        pool: Arc<sqlx::SqlitePool>,
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessFeeTxReportCommand>,
    ) -> Self {
        let worker_ctx = FeeTxWorkerCtx {
            pool: pool.clone(),
            address_locks: Arc::new(DashMap::new()),
            global_sem: Arc::new(Semaphore::new(64)),
        };

        Self { shutdown_rx, report_rx, worker_ctx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!("starting process fee tx report -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process fee tx report -------------------------------");
                    break;
                }
                report_msg = self.report_rx.recv() => {
                    if let Some(cmd) = report_msg {
                        match cmd {
                            ProcessFeeTxReportCommand::Tx(trade_no) => {
                                self.process_fee_single_tx_report_by_trade_no(&trade_no);
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                    self.process_fee_tx_report()
                }
            }
        }
        tracing::info!("closing process fee tx report ------------------------------- end");
    }

    fn process_fee_single_tx_report_by_trade_no(&self, trade_no: &str) {
        let ctx = self.worker_ctx.clone();
        let trade_no = trade_no.to_string();
        tracing::info!(trade_no=%trade_no, "[手续费归集报告] 根据交易编号处理单个手续费交易报告");
        tokio::spawn(async move {
            let api_fee = match ApiFeeRepo::get_api_fee_by_trade_no_status(
                &ctx.pool,
                &trade_no,
                &[
                    ApiFeeStatus::SendingTx,
                    ApiFeeStatus::SendingTxFailed,
                    ApiFeeStatus::SendingTxReport,
                    ApiFeeStatus::Success,
                    ApiFeeStatus::Failure,
                ],
            )
            .await
            {
                Ok(v) => v,
                Err(err) => {
                    tracing::warn!(trade_no=%trade_no, "[手续费归集报告] 获取手续费交易报告失败: {}", err);
                    return;
                }
            };
            tracing::info!(trade_no=%trade_no, "[手续费归集报告] 找到待处理的手续费交易报告");
            let lock = ctx.get_address_lock(&api_fee.from_addr);
            let _guard = lock.lock().await;
            let _permit = ctx.global_sem.acquire().await.unwrap();
            // 直接调用时不检查重试时间
            Self::process_fee_single_tx_report(ctx.pool, api_fee, false).await
        });
    }

    fn process_fee_tx_report(&mut self) {
        tracing::info!("[手续费归集报告] 批量处理手续费交易报告");
        let ctx = self.worker_ctx.clone();

        tokio::spawn(async move {
            let res = ApiFeeRepo::page_api_fee_with_status(
                &ctx.pool,
                0,
                1000,
                &[
                    ApiFeeStatus::SendingTx,
                    ApiFeeStatus::SendingTxFailed,
                    ApiFeeStatus::SendingTxReport,
                    ApiFeeStatus::Success,
                    ApiFeeStatus::Failure,
                ],
            )
            .await;
            let (_, transfer_fees) = match res {
                Ok(v) => v,
                Err(err) => {
                    tracing::warn!("[手续费归集报告] 获取手续费交易报告列表失败: {}", err);
                    return;
                }
            };

            tracing::info!(
                "[手续费归集报告] 找到 {} 条待处理的手续费交易报告",
                transfer_fees.len()
            );
            // 使用并发处理，但确保同一地址的交易串行处理
            for req in transfer_fees {
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    let lock = ctx.get_address_lock(&req.from_addr);
                    let _guard = lock.lock().await;
                    let _permit = ctx.global_sem.acquire().await.unwrap();
                    // 定时检查时需要检查重试时间
                    Self::process_fee_single_tx_report(ctx.pool.clone(), req, true).await
                });
            }
        });
    }

    /// 处理单个手续费交易报告的辅助函数，用于并发处理
    async fn process_fee_single_tx_report(
        pool: Arc<sqlx::SqlitePool>,
        req: ApiFeeEntity,
        check_retry_time: bool,
    ) {
        tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 处理单个手续费交易报告，当前状态: {:?}", req.status);

        // 终态直接返回
        match req.status {
            ApiFeeStatus::ConfirmSuccessReport | ApiFeeStatus::ConfirmFailureReport => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 已处于终态，跳过");
                return;
            }
            _ => {}
        }

        // 只有在需要检查重试时间时才执行检查
        if check_retry_time {
            // 判断超时时间
            let now = chrono::Utc::now();
            let timeout = now - req.updated_at.unwrap();
            let max_backoff = 60; // 60秒
            let backoff = (1i64 << req.post_tx_count).clamp(1, max_backoff);
            tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 当前时间: {}, 上次更新时间: {}, 超时时间: {}, 当前重试次数: {}, 退避时间: {}秒", 
                        now, req.updated_at.unwrap(), timeout, req.post_tx_count, backoff);

            if timeout < TimeDelta::seconds(backoff) {
                tracing::warn!(trade_no=%req.trade_no, "[手续费归集报告] 未到重试时间，跳过本次处理");
                return;
            }
        } else {
            tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 直接调用，跳过重试时间检查");
        }

        // 根据当前状态确定上传类型
        let (status, remark, trans_type) = match req.status {
            // 第一阶段：发送交易报告
            ApiFeeStatus::SendingTx | ApiFeeStatus::SendingTxFailed => {
                if req.status == ApiFeeStatus::SendingTxFailed {
                    tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 交易发送失败，准备上传失败报告");
                    let msg = json!({
                        "code": req.err_code,
                        "msg": req.err_msg,
                    });
                    let s = msg.to_string();
                    tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 失败报告内容: {}", s);
                    (TransStatus::Fail, s, TransType::ColFee)
                } else {
                    tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 交易发送成功，准备上传成功报告");
                    (TransStatus::Success, "".to_string(), TransType::ColFee)
                }
            }
            // 第二阶段：确认报告
            ApiFeeStatus::Success | ApiFeeStatus::Failure => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 处理确认报告，状态: {:?}", req.status);
                if req.status == ApiFeeStatus::Failure {
                    let msg = json!({
                        "code": req.err_code,
                        "msg": req.err_msg,
                    });
                    let s = msg.to_string();
                    (TransStatus::Fail, s, TransType::ColFee)
                } else {
                    (TransStatus::Success, "".to_string(), TransType::ColFee)
                }
            }
            // 报告已发送状态，跳过
            ApiFeeStatus::SendingTxReport | ApiFeeStatus::SendingTxFailedReport => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 报告已发送，跳过本次处理");
                return;
            }
            _ => {
                tracing::warn!(trade_no=%req.trade_no, "[手续费归集报告] 未知状态: {:?}，跳过", req.status);
                return;
            }
        };

        tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 调用后端API上传交易执行报告");
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        match backend_api
            .upload_tx_exec_receipt(&TxExecReceiptUploadReq::new(
                None,
                None,
                &req.trade_no,
                trans_type,
                Some(&req.tx_hash),
                status,
                remark.as_str(),
            ))
            .await
        {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 交易执行报告上传成功");
                Self::handle_report_success(pool.clone(), req).await;
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[手续费归集报告] 交易执行报告上传失败: {}", err);
                Self::handle_report_failed(pool.clone(), req, err).await;
            }
        }
    }

    /// 处理交易执行报告上传成功的辅助函数
    async fn handle_report_success(pool: Arc<sqlx::SqlitePool>, req: ApiFeeEntity) {
        tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 处理交易执行报告上传成功");

        // 获取当前状态，防止状态覆盖
        let current = match ApiFeeRepo::get_api_fee_by_trade_no(&pool, &req.trade_no).await {
            Ok(cur) => cur,
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[手续费归集报告] 获取当前手续费状态失败: {}", err);
                return;
            }
        };

        // 检查是否已处于终态，防止状态回退
        match current.status {
            ApiFeeStatus::Success
            | ApiFeeStatus::ConfirmSuccessReport
            | ApiFeeStatus::ConfirmFailureReport
            | ApiFeeStatus::Failure => {
                tracing::warn!(trade_no=%req.trade_no, "[手续费归集报告] 当前已进入终态 {:?}，忽略旧报告", current.status);
                return;
            }
            _ => {}
        }

        // 根据当前状态确定下一状态
        let (next_status, notes) = match current.status {
            ApiFeeStatus::SendingTxFailed => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 交易发送失败报告上传成功，更新状态为SendingTxFailedReport");
                (
                    ApiFeeStatus::SendingTxFailedReport,
                    "upload server ok for transfer fee send tx failed",
                )
            }
            ApiFeeStatus::SendingTx => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 交易发送成功报告上传成功，更新状态为SendingTxReport");
                (ApiFeeStatus::SendingTxReport, "upload server ok for transfer fee success")
            }
            ApiFeeStatus::Success => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 确认成功报告上传成功，更新状态为ConfirmSuccessReport");
                (
                    ApiFeeStatus::ConfirmSuccessReport,
                    "upload server ok for transfer fee confirm success",
                )
            }
            ApiFeeStatus::Failure => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 确认失败报告上传成功，更新状态为ConfirmFailureReport");
                (
                    ApiFeeStatus::ConfirmFailureReport,
                    "upload server ok for transfer fee confirm failure",
                )
            }
            _ => {
                tracing::warn!(trade_no=%req.trade_no, "[手续费归集报告] 当前状态 {:?} 不支持报告上传成功处理，跳过", current.status);
                return;
            }
        };

        // 使用当前状态进行 CAS 更新，防止状态覆盖
        let res = ApiFeeRepo::update_api_fee_next_status(
            &pool,
            &req.trade_no,
            current.status,
            next_status,
        )
        .await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 交易报告状态更新成功: {}", notes);
            }
            Err(_) => {
                tracing::error!(trade_no=%req.trade_no, "[手续费归集报告] 交易报告状态更新失败，可能状态已被其他线程修改");
            }
        }
    }

    /// 处理交易执行报告上传失败的辅助函数
    async fn handle_report_failed(
        pool: Arc<sqlx::SqlitePool>,
        req: ApiFeeEntity,
        err: wallet_transport_backend::Error,
    ) {
        tracing::error!(trade_no=%req.trade_no, "[手续费归集报告] 处理交易执行报告上传失败: {}", err);
        tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 更新手续费交易报告重试次数");
        let res = ApiFeeRepo::update_api_fee_post_tx_count(&pool, &req.trade_no, req.status).await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集报告] 手续费交易报告重试次数更新成功");
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "[手续费归集报告] 手续费交易报告重试次数更新失败: {:?}", err);
            }
        }
    }
}
