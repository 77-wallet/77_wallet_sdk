// process_withdraw_tx_confirm.rs
use crate::infrastructure::withdraw::command::ProcessWithdrawTxConfirmReportCommand;
use chrono::TimeDelta;
use dashmap::DashMap;
use std::sync::{Arc, Weak};
use tokio::{
    sync::{Mutex, Semaphore, broadcast, mpsc},
    time::sleep,
};
use wallet_database::{
    CollectDbPool,
    entities::{
        api_trade_type::ApiTradeType,
        api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
    },
    repositories::api_wallet::withdraw::ApiWithdrawRepo,
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

#[derive(Clone)]
struct WithdrawConfirmWorkerCtx {
    pool: CollectDbPool,
    trade_locks: Arc<DashMap<String, Weak<Mutex<()>>>>,
    address_locks: Arc<DashMap<String, Weak<Mutex<()>>>>,
    global_sem: Arc<Semaphore>,
}

impl WithdrawConfirmWorkerCtx {
    fn get_trade_lock(&self, trade_no: &str) -> Arc<Mutex<()>> {
        Self::get_lock(&self.trade_locks, trade_no)
    }

    fn get_address_lock(&self, address: &str) -> Arc<Mutex<()>> {
        Self::get_lock(&self.address_locks, address)
    }

    fn get_lock(map: &Arc<DashMap<String, Weak<Mutex<()>>>>, key: &str) -> Arc<Mutex<()>> {
        if let Some(entry) = map.get(key) {
            if let Some(lock) = entry.value().upgrade() {
                return lock;
            }
        }
        let lock = Arc::new(Mutex::new(()));
        map.insert(key.to_string(), Arc::downgrade(&lock));
        lock
    }
}

pub(super) struct ProcessWithdrawTxConfirmReport {
    shutdown_rx: broadcast::Receiver<()>,
    report_rx: mpsc::Receiver<ProcessWithdrawTxConfirmReportCommand>,
    worker_ctx: WithdrawConfirmWorkerCtx,
}

impl ProcessWithdrawTxConfirmReport {
    pub(super) fn new(
        pool: CollectDbPool,
        shutdown_rx: broadcast::Receiver<()>,
        report_rx: mpsc::Receiver<ProcessWithdrawTxConfirmReportCommand>,
    ) -> Self {
        let worker_ctx = WithdrawConfirmWorkerCtx {
            pool,
            trade_locks: Arc::new(DashMap::new()),
            address_locks: Arc::new(DashMap::new()),
            global_sem: Arc::new(Semaphore::new(10)),
        };

        Self { shutdown_rx, report_rx, worker_ctx }
    }

    pub(super) async fn run(&mut self) {
        tracing::info!(
            "starting process withdraw tx confirm report -------------------------------"
        );
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            let res = GLOBAL_KEY.is_exchange_shared_secret();
            if res.is_err() {
                sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process withdraw tx confirm report -------------------------------");
                    break;
                }
                msg = self.report_rx.recv() => {
                    if let Some(cmd) = msg {
                        match cmd {
                            ProcessWithdrawTxConfirmReportCommand::Tx(trade_no) => {
                                self.spawn_single(&trade_no);
                                iv.reset();
                            }
                        }
                    }
                }
                _ = iv.tick() => {
                     self.spawn_batch()
                }
            }
        }
        tracing::info!(
            "closing process withdraw tx confirm report ------------------------------- end"
        );
    }

    fn spawn_single(&self, trade_no: &str) {
        let ctx = self.worker_ctx.clone();
        let trade_no = trade_no.to_string();

        tracing::info!(trade_no=%trade_no, "[提现确认] 根据交易编号处理单个提现交易确认报告");
        tokio::spawn(async move {
            match ApiWithdrawRepo::get_api_withdraw_by_trade_no_status(
                &ctx.pool,
                &trade_no,
                &[ApiWithdrawStatus::Failure, ApiWithdrawStatus::Success],
            )
            .await
            {
                Ok(req) => {
                    tracing::info!(trade_no=%trade_no, "[提现确认] 找到待处理的提现交易确认报告");

                    // lock order: trade -> address -> global
                    let trade_lock = ctx.get_trade_lock(&trade_no);
                    let address_lock = ctx.get_address_lock(&req.to_addr);
                    let _trade_guard = trade_lock.lock().await;
                    let _address_guard = address_lock.lock().await;
                    let _permit = ctx.global_sem.acquire().await.unwrap();

                    Self::process_withdraw_single_tx_confirm_report(ctx.pool.clone(), req).await;
                }
                Err(err) => {
                    tracing::warn!(trade_no=%trade_no, "[提现确认] 获取提现交易确认报告失败: {}", err);
                }
            }
        });
    }

    fn spawn_batch(&mut self) {
        let ctx = self.worker_ctx.clone();

        tracing::info!("[提现确认] 批量处理提现交易确认报告");

        tokio::spawn(async move {
            let res = ApiWithdrawRepo::list_api_withdraw_with_status(
                &ctx.pool,
                vec![ApiWithdrawStatus::Failure, ApiWithdrawStatus::Success],
                0,
                1000,
            )
            .await;
            let withdraws = match res {
                Ok(withdraws) => withdraws,
                Err(err) => {
                    tracing::warn!("[提现确认] 获取提现交易确认报告列表失败: {}", err);
                    return;
                }
            };
            tracing::info!("[提现确认] 找到 {} 条待处理的提现交易确认报告", withdraws.len());
            for req in withdraws {
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    let trade_lock = ctx.get_trade_lock(&req.trade_no);
                    let address_lock = ctx.get_address_lock(&req.to_addr);
                    let _trade_guard = trade_lock.lock().await;
                    let _address_guard = address_lock.lock().await;
                    let _permit = ctx.global_sem.acquire().await.unwrap();

                    Self::process_withdraw_single_tx_confirm_report(ctx.pool.clone(), req).await
                });
            }
        });
    }

    async fn process_withdraw_single_tx_confirm_report(
        pool: CollectDbPool,
        req: ApiWithdrawEntity,
    ) {
        tracing::info!(trade_no=%req.trade_no,status=%req.status, "process_withdraw_single_tx_confirm_report ---------------------------------4");
        let now = chrono::Utc::now();
        let timeout = now - req.updated_at.unwrap();
        if timeout < TimeDelta::seconds(req.post_confirm_tx_count as i64) {
            tracing::warn!(trade_no=%req.trade_no,
                "process_withdraw_single_tx_confirm_report timeout post confirm_tx_count is too long"
            );
            return;
        }
        if req.status == ApiWithdrawStatus::SendingTxFailed {
            tracing::warn!(trade_no=%req.trade_no, "process_withdraw_single_tx_confirm_report status is wrong");
            return;
        };
        if !(req.status == ApiWithdrawStatus::Success || req.status == ApiWithdrawStatus::Failure) {
            tracing::warn!(trade_no=%req.trade_no,
                "process_withdraw_single_tx_confirm_report status is wrong {}",
                req.status
            );
            return;
        }

        // 添加幂等性检查，防止重复发送 Result ACK
        let (_, result_ack_sent_at) =
            ApiWithdrawRepo::get_ack_times(&pool, &req.trade_no).await.unwrap_or((None, None));
        if result_ack_sent_at.is_some() {
            tracing::warn!(trade_no=%req.trade_no, ?result_ack_sent_at, "[提现确认] Result ACK 已发送，跳过");
            return;
        }

        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        tracing::info!(trade_no=%req.trade_no, "[提现确认] 准备调用后端API发送 Result ACK");

        match backend_api
            .trans_event_ack(&TransEventAckReq::new(
                &req.trade_no,
                TransType::Wd,
                TransAckType::TxRes,
            ))
            .await
        {
            Ok(_) => {
                tracing::info!(trade_no=%req.trade_no, "[提现确认] 发送 TxRes ACK 成功");

                // 设置 TxRes ACK 发送时间
                if let Err(e) = ApiWithdrawRepo::set_tx_res_ack_sent(&pool, &req.trade_no).await {
                    tracing::error!(trade_no=%req.trade_no, "[提现确认] 设置 TxRes ACK 发送时间失败: {}", e);
                } else {
                    tracing::info!(trade_no=%req.trade_no, "[提现确认] 设置 TxRes ACK 发送时间成功");
                }

                Self::handle_confirm_report_success(pool.clone(), req).await
            }
            Err(err) => {
                tracing::error!(trade_no=%req.trade_no, "[提现确认] 发送 TxRes ACK 失败: {}", err);
                Self::handle_confirm_report_failed(pool.clone(), req, err).await
            }
        }
    }

    async fn handle_confirm_report_success(pool: CollectDbPool, req: ApiWithdrawEntity) {
        let withdraw = match ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &pool,
            &req.trade_no,
            ApiTradeType::Withdraw,
        )
        .await
        {
            Ok(withdraw) => withdraw,
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "failed to get withdraw by trade no: {:?}", err);
                return;
            }
        };
        if withdraw.status >= ApiWithdrawStatus::ConfirmSuccessReport {
            tracing::info!(trade_no=%req.trade_no, "withdraw already finished, skip");
            return;
        }
        let (next_status, _notes) = if withdraw.status == ApiWithdrawStatus::Success {
            (ApiWithdrawStatus::ConfirmSuccessReport, "withdraw trans event ack success")
        } else {
            (ApiWithdrawStatus::ConfirmFailureReport, "withdraw trans event ack failure")
        };
        tracing::info!(trade_no=%req.trade_no, "process_withdraw_single_tx_confirm_report success");
        let res =
            ApiWithdrawRepo::update_api_withdraw_status(&pool, &req.trade_no, next_status).await;
        match res {
            Ok(res) => {
                if res != 1 {
                    tracing::warn!(trade_no=%req.trade_no, "failed to process withdraw tx confirm: {:?}", res);
                }
            }
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "process withdraw single tx report by id: {:?}", err);
            }
        }
    }

    async fn handle_confirm_report_failed(
        pool: CollectDbPool,
        req: ApiWithdrawEntity,
        err: wallet_transport_backend::Error,
    ) {
        tracing::error!(trade_no=%req.trade_no, "failed to process withdraw tx confirm report: {}", err);
        let res = ApiWithdrawRepo::update_api_withdraw_post_confirm_tx_count(
            &pool,
            &req.trade_no,
            req.status,
        )
        .await;
        match res {
            Ok(_res) => {}
            Err(err) => {
                tracing::warn!(trade_no=%req.trade_no, "process withdraw tx report by id: {:?}", err);
            }
        }
    }
}
