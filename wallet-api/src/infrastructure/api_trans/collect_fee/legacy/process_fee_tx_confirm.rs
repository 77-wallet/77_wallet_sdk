// legacy collect fee transaction confirm worker.
#![allow(deprecated)]

use crate::infrastructure::api_trans::collect_fee::command::ProcessFeeTxConfirmReportCommand;
use chrono::TimeDelta;
use dashmap::DashMap;
use std::sync::{Arc, Weak};
use tokio::{
    sync::{Mutex, Semaphore, broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    ApiTransactionDbPool,
    entities::api_fee::{ApiFeeEntity, ApiFeeStatus},
    repositories::api_wallet::fee::ApiFeeRepo,
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

#[derive(Clone)]
struct FeeConfirmWorkerCtx {
    ctx: &'static crate::context::Context,
    address_locks: Arc<DashMap<String, Weak<Mutex<()>>>>,
    global_sem: Arc<Semaphore>,
}

impl FeeConfirmWorkerCtx {
    fn get_address_lock(&self, address: &str) -> Arc<Mutex<()>> {
        if let Some(entry) = self.address_locks.get(address) {
            if let Some(lock) = entry.value().upgrade() {
                return lock;
            }
        }

        let lock = Arc::new(Mutex::new(()));
        self.address_locks.insert(address.to_string(), Arc::downgrade(&lock));
        lock
    }
}
pub(super) struct ProcessFeeTxConfirmReport {
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessFeeTxConfirmReportCommand>,
    worker_ctx: FeeConfirmWorkerCtx,
}

impl ProcessFeeTxConfirmReport {
    pub(super) fn new(
        ctx: &'static crate::context::Context,
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessFeeTxConfirmReportCommand>,
    ) -> Self {
        let worker_ctx = FeeConfirmWorkerCtx {
            ctx,
            address_locks: Arc::new(DashMap::new()),
            global_sem: Arc::new(Semaphore::new(10)),
        };
        Self { shutdown_rx, report_rx, worker_ctx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!("starting process fee tx confirm report -------------------------------");
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process fee tx confirm report -------------------------------");
                    break;
                }
                report_msg = self.report_rx.recv() => {
                    match report_msg {
                        Some(cmd) => {
                            match cmd {
                                ProcessFeeTxConfirmReportCommand::Tx(trade_no) => {
                                    self.spawn_single(&trade_no);
                                    iv.reset();
                                }
                            }
                        }
                        None => {}
                    }
                }
                _ = iv.tick() => {
                    self.spawn_batch();
                }
            }
        }
        tracing::info!("closing process fee tx confirm report ------------------------------- end");
    }

    fn spawn_single(&self, trade_no: &str) {
        let ctx = self.worker_ctx.clone();
        let trade_no = trade_no.to_string();

        tracing::info!(trade_no=%trade_no, "[手续费归集确认] 根据交易编号处理单个手续费交易确认报告");

        tokio::spawn(async move {
            let pool = match ctx.ctx.api_transaction_pool() {
                Ok(pool) => pool,
                Err(err) => {
                    tracing::warn!(trade_no=%trade_no, "[手续费归集确认] 获取交易数据库连接池失败: {}", err);
                    return;
                }
            };
            match ApiFeeRepo::get_api_fee_by_trade_no(&pool, &trade_no).await {
                Ok(fee) => {
                    tracing::info!(trade_no=%trade_no, "[手续费归集确认] 找到待处理的手续费交易确认报告");
                    let lock = ctx.get_address_lock(&fee.from_addr);
                    let _guard = lock.lock().await;
                    let _permit = ctx.global_sem.acquire().await.unwrap();

                    Self::process_fee_single_tx_confirm_report(fee, ctx.ctx).await;
                }
                Err(err) => {
                    tracing::warn!(trade_no=%trade_no, "[手续费归集确认] 获取手续费交易确认报告失败: {}", err);
                }
            }
        });
    }

    fn spawn_batch(&mut self) {
        let ctx = self.worker_ctx.clone();

        tracing::info!("[手续费归集确认] 批量处理手续费交易确认报告");

        tokio::spawn(async move {
            let pool = match ctx.ctx.api_transaction_pool() {
                Ok(pool) => pool,
                Err(err) => {
                    tracing::warn!("[手续费归集确认] 获取交易数据库连接池失败: {}", err);
                    return;
                }
            };
            let res = ApiFeeRepo::page_api_fee_with_status(
                &pool,
                0,
                1000,
                &[ApiFeeStatus::Failure, ApiFeeStatus::Success],
            )
            .await;
            let (_, transfer_fees) = match res {
                Ok((total, transfer_fees)) => (total, transfer_fees),
                Err(err) => {
                    tracing::warn!("[手续费归集确认] 获取手续费交易确认报告列表失败: {}", err);
                    return;
                }
            };
            tracing::info!(
                "[手续费归集确认] 找到 {} 条待处理的手续费交易确认报告",
                transfer_fees.len()
            );
            for req in transfer_fees {
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    let lock = ctx.get_address_lock(&req.from_addr);
                    let _guard = lock.lock().await;
                    let _permit = ctx.global_sem.acquire().await.unwrap();

                    Self::process_fee_single_tx_confirm_report(req, ctx.ctx).await
                });
            }
        });
    }

    async fn process_fee_single_tx_confirm_report(
        req: ApiFeeEntity,
        ctx: &'static crate::context::Context,
    ) {
        tracing::info!(trade_no=%req.trade_no,hash=?req.tx_hash,status=%req.status, "[手续费归集确认] 处理单个手续费交易确认报告");
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(req.post_confirm_tx_count as i64) {
            tracing::warn!(trade_no=%req.trade_no,
                "[手续费归集确认] 手续费交易确认报告处理超时，retry not due yet, skip this round"
            );
            return;
        }
        if req.status == ApiFeeStatus::SendingTxFailed {
            tracing::warn!(trade_no=%req.trade_no, "[手续费归集确认] 手续费交易确认报告状态错误");
            return;
        };
        if !(req.status == ApiFeeStatus::Success || req.status == ApiFeeStatus::Failure) {
            tracing::warn!(trade_no=%req.trade_no, "[手续费归集确认] 手续费交易确认报告状态错误: {}", req.status);
            return;
        }
        tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 调用后端API发送交易确认报告");
        // 检查 TxRes ACK 是否已发送
        let pool = match ctx.api_transaction_pool() {
            Ok(pool) => pool,
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "[手续费归集确认] 获取交易数据库连接池失败: {}", err);
                return;
            }
        };
        let (_, tx_res_ack_sent_at) =
            ApiFeeRepo::get_ack_times(&pool, &req.trade_no).await.unwrap_or((None, None));
        if tx_res_ack_sent_at.is_some() {
            tracing::warn!(trade_no=%req.trade_no, ?tx_res_ack_sent_at, "[手续费归集确认] TxRes ack 已发送，跳过");
            return;
        }

        let backend_api = ctx.get_global_backend_api();
        match backend_api
            .trans_event_ack(&TransEventAckReq::new(
                &req.trade_no,
                TransType::ColFee,
                TransAckType::TxRes,
            ))
            .await
        {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 交易确认报告发送成功");
                // 设置 TxRes ACK 发送时间
                if let Err(err) = ApiFeeRepo::set_tx_res_ack_sent(&pool, &req.trade_no).await {
                    tracing::error!(trade_no=%req.trade_no, "[手续费归集确认] 设置 TxRes ACK 发送时间失败: {}", err);
                }
                Self::handle_confirm_report_success(pool.clone(), req).await;
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[手续费归集确认] 交易确认报告发送失败: {}", err);
                Self::handle_confirm_report_failed(pool.clone(), req, err).await;
            }
        }
    }

    async fn handle_confirm_report_success(pool: ApiTransactionDbPool, req: ApiFeeEntity) {
        tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 处理交易确认报告发送成功");
        let next_status = if req.status == ApiFeeStatus::Success {
            tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 交易成功，更新状态为ConfirmSuccessReport");
            ApiFeeStatus::ConfirmSuccessReport
        } else {
            tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 交易失败，更新状态为ConfirmFailureReport");
            ApiFeeStatus::ConfirmFailureReport
        };

        let res =
            ApiFeeRepo::update_api_fee_next_status(&pool, &req.trade_no, req.status, next_status)
                .await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 交易确认报告状态更新成功");
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "[手续费归集确认] 交易确认报告状态更新失败: {}", err);
            }
        }
    }

    async fn handle_confirm_report_failed(
        pool: ApiTransactionDbPool,
        req: ApiFeeEntity,
        err: wallet_transport_backend::Error,
    ) {
        tracing::error!(trade_no=%req.trade_no, "[手续费归集确认] 处理交易确认报告发送失败: {}", err);
        tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 更新手续费交易确认报告重试次数");
        let res =
            ApiFeeRepo::update_api_fee_post_confirm_tx_count(&pool, &req.trade_no, req.status)
                .await;
        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[手续费归集确认] 手续费交易确认报告重试次数更新成功");
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "[手续费归集确认] 手续费交易确认报告重试次数更新失败: {}", err);
            }
        }
    }
}
