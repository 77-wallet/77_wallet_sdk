#![allow(deprecated)]

// legacy collect transaction confirm worker.
use crate::infrastructure::api_trans::collect::command::ProcessCollectTxConfirmReportCommand;
use chrono::TimeDelta;
use dashmap::DashMap;
use std::sync::{Arc, Weak};
use tokio::{
    sync::{Mutex, Semaphore, broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    ApiTransactionDbPool,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::collect::ApiCollectRepo,
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

#[derive(Clone)]
struct CollectConfirmWorkerCtx {
    pool: ApiTransactionDbPool,
    address_locks: Arc<DashMap<String, Weak<Mutex<()>>>>,
    global_sem: Arc<Semaphore>,
}

impl CollectConfirmWorkerCtx {
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

pub(super) struct ProcessCollectTxConfirmReport {
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessCollectTxConfirmReportCommand>,
    worker_ctx: CollectConfirmWorkerCtx,
}

impl ProcessCollectTxConfirmReport {
    pub(super) fn new(
        pool: ApiTransactionDbPool,
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessCollectTxConfirmReportCommand>,
    ) -> Self {
        let worker_ctx = CollectConfirmWorkerCtx {
            pool,
            address_locks: Arc::new(DashMap::new()),
            global_sem: Arc::new(Semaphore::new(64)),
        };

        Self { shutdown_rx, report_rx, worker_ctx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!(
            "starting process collect tx confirm report -------------------------------"
        );
        self.run_with_err().await;
        tracing::info!("closing process collect tx confirm report ------------- end");
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
                    tracing::info!("closing process collect tx confirm report -------------------------------");
                    break;
                }
                report_msg = self.report_rx.recv() => {
                    match report_msg {
                        Some(cmd) => {
                            match cmd {
                                ProcessCollectTxConfirmReportCommand::Tx(trade_no) => {
                                    self.spawn_single(&trade_no);
                                    iv.reset();
                                }
                            }
                        }
                        None => {}
                    }
                }
                _ = iv.tick() => {
                    self.spawn_batch()
                }
            }
        }
    }

    fn spawn_single(&self, trade_no: &str) {
        let ctx = self.worker_ctx.clone();
        let trade_no = trade_no.to_string();

        tracing::info!(trade_no=%trade_no, "[归集交易确认] 开始处理单个归集交易确认报告");

        tokio::spawn(async move {
            let req = match ApiCollectRepo::get_api_collect_by_trade_no_status(
                &ctx.pool,
                &trade_no,
                &[ApiCollectStatus::Failure, ApiCollectStatus::Success],
            )
            .await
            {
                Ok(v) => v,
                Err(err) => {
                    tracing::warn!(
                        trade_no = %trade_no,
                        "[归集交易确认] 查询交易信息失败: {}",
                        err
                    );
                    return;
                }
            };
            tracing::info!(trade_no=%trade_no, status=%req.status, "[归集交易确认] 查询到交易信息，开始处理确认报告");
            let lock = ctx.get_address_lock(&req.from_addr);
            let _guard = lock.lock().await;
            let _permit = ctx.global_sem.acquire().await.unwrap();

            Self::process_collect_single_tx_confirm_report(ctx.pool.clone(), req, false).await
        });
    }

    fn spawn_batch(&mut self) {
        let ctx = self.worker_ctx.clone();

        tracing::info!("[归集交易确认] 开始批量处理归集交易确认报告");

        tokio::spawn(async move {
            let res = ApiCollectRepo::page_api_collect_with_status(
                &ctx.pool,
                0,
                1000,
                &[ApiCollectStatus::Failure, ApiCollectStatus::Success],
            )
            .await;
            let (_, collects) = match res {
                Ok(v) => v,
                Err(err) => {
                    tracing::warn!("[归集交易确认] 批量查询交易信息失败: {}", err);
                    return;
                }
            };
            tracing::info!("[归集交易确认] 查询到 {} 笔待处理的归集交易确认报告", collects.len());
            for req in collects {
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    let lock = ctx.get_address_lock(&req.from_addr);
                    let _guard = lock.lock().await;
                    let _permit = ctx.global_sem.acquire().await.unwrap();

                    Self::process_collect_single_tx_confirm_report(ctx.pool.clone(), req, true)
                        .await
                });
            }
        });
    }

    async fn process_collect_single_tx_confirm_report(
        pool: ApiTransactionDbPool,
        req: ApiCollectEntity,
        check_retry_time: bool,
    ) {
        tracing::info!(trade_no=%req.trade_no, status=%req.status, "[归集交易确认] 开始处理单条归集交易确认报告");

        // 只有在需要检查重试时间时才执行检查
        if check_retry_time {
            let now = chrono::Utc::now();
            let timeout = now - req.updated_at.unwrap();
            tracing::info!(trade_no=%req.trade_no, "[归集交易确认] 当前时间: {}, 上次更新时间: {}, 超时时间: {}, 当前重试次数: {}", 
                         now, req.updated_at.unwrap(), timeout, req.post_confirm_tx_count);

            if timeout < TimeDelta::seconds(req.post_confirm_tx_count as i64) {
                tracing::warn!(trade_no=%req.trade_no,
                    "[归集交易确认] 未到重试时间，跳过本次处理，当前重试次数: {}", req.post_confirm_tx_count
                );
                return;
            }
        } else {
            tracing::info!(trade_no=%req.trade_no, "[归集交易确认] 直接调用，跳过重试时间检查");
        }
        if req.status == ApiCollectStatus::SendingTxFailed {
            tracing::warn!(trade_no=%req.trade_no, "[归集交易确认] 交易状态错误: SendingTxFailed");
            return;
        };
        if !(req.status == ApiCollectStatus::Success || req.status == ApiCollectStatus::Failure) {
            tracing::warn!(
                trade_no=%req.trade_no,
                "[归集交易确认] 交易状态错误: {}",
                req.status
            );
            return;
        }

        let backend_api = crate::get_context()?.get_global_backend_api();
        tracing::info!(trade_no=%req.trade_no, "[归集交易确认] 准备调用后端API发送交易事件确认");

        // 检查 TxRes ACK 是否已发送
        let (_, result_ack_sent_at) =
            ApiCollectRepo::get_ack_times(&pool, &req.trade_no).await.unwrap_or((None, None));
        if result_ack_sent_at.is_some() {
            tracing::warn!(trade_no=%req.trade_no, ?result_ack_sent_at, "[归集交易确认] Result ack 已发送，跳过");
            return;
        }

        match backend_api
            .trans_event_ack(&TransEventAckReq::new(
                &req.trade_no,
                TransType::Col,
                TransAckType::TxRes,
            ))
            .await
        {
            Ok(_) => {
                // TODO：设置ACK时间
                tracing::info!(trade_no=%req.trade_no, "[归集交易确认] 发送交易事件确认成功");

                Self::handle_confirm_report_success(pool.clone(), req).await
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "[归集交易确认] 发送交易事件确认失败: {}", err);
                Self::handle_confirm_report_failed(pool, req, err).await
            }
        }
    }

    async fn handle_confirm_report_success(pool: ApiTransactionDbPool, req: ApiCollectEntity) {
        let (next_status, _notes) = if req.status == ApiCollectStatus::Success {
            tracing::info!(trade_no=%req.trade_no, "[归集交易确认] 交易确认报告上传成功，准备更新状态为ConfirmSuccessReport");
            (ApiCollectStatus::ConfirmSuccessReport, "trans event ack success")
        } else {
            tracing::info!(trade_no=%req.trade_no, "[归集交易确认] 交易失败确认报告上传成功，准备更新状态为ConfirmFailureReport");
            (ApiCollectStatus::ConfirmFailureReport, "trans event ack failed")
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
                tracing::info!(trade_no=%req.trade_no, "[归集交易确认] 更新交易状态成功，新状态: {}", next_status);
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[归集交易确认] 更新交易状态失败: {}", err);
            }
        }
    }

    async fn handle_confirm_report_failed(
        pool: ApiTransactionDbPool,
        req: ApiCollectEntity,
        err: wallet_transport_backend::Error,
    ) {
        tracing::warn!(trade_no=%req.trade_no, "[归集交易确认] 发送确认报告失败，准备增加重试次数: {}", err);
        let res =
            ApiCollectRepo::update_api_collect_post_confirm_tx_count(&pool, &req.trade_no).await;

        match res {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[归集交易确认] 增加重试次数成功，当前重试次数: {}", req.post_confirm_tx_count + 1);
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[归集交易确认] 更新重试次数失败: {}", err);
            }
        }
    }
}
