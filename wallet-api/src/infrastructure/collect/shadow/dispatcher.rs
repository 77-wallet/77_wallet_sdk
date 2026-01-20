use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashSet;
use sqlx::SqlitePool;
use tokio::sync::Semaphore;
use tracing::{info, warn, debug};

use wallet_database::entities::api_collect::ApiCollectStatus;
use wallet_database::repositories::api_wallet::collect::ApiCollectRepo;

use super::CollectIntent;

/// Shadow Dispatcher 配置
#[derive(Debug, Clone)]
pub struct DispatcherConfig {
    /// 全局并发控制信号量大小
    pub semaphore_size: usize,
    /// 二次校验超时时间
    pub db_check_timeout: Duration,
}

impl Default for DispatcherConfig {
    fn default() -> Self {
        Self {
            semaphore_size: 100,
            db_check_timeout: Duration::from_secs(5), // 5秒
        }
    }
}

/// Shadow Dispatcher
/// 
/// 负责：
/// 1. 防止并发重复执行同一trade_no
/// 2. 控制全局吞吐
/// 3. DB状态二次校验
/// 4. 决策是否推进状态
pub struct ShadowDispatcher {
    pool: Arc<SqlitePool>,
    config: DispatcherConfig,
    /// 正在执行的trade_no集合，防止并发重复执行
    running: DashSet<String>,
    /// 全局并发控制信号量
    semaphore: Arc<Semaphore>,
    /// 原有系统的命令通道
    tx_tx: tokio::sync::mpsc::Sender<crate::infrastructure::collect::command::ProcessCollectTxCommand>,
    /// 原有系统的报告通道
    report_tx: tokio::sync::mpsc::Sender<crate::infrastructure::collect::command::ProcessCollectTxReportCommand>,
    /// 原有系统的确认报告通道
    confirm_report_tx: tokio::sync::mpsc::Sender<crate::infrastructure::collect::command::ProcessCollectTxConfirmReportCommand>,
}

impl ShadowDispatcher {
    pub fn new(
        pool: Arc<SqlitePool>, 
        config: DispatcherConfig,
        tx_tx: tokio::sync::mpsc::Sender<crate::infrastructure::collect::command::ProcessCollectTxCommand>,
        report_tx: tokio::sync::mpsc::Sender<crate::infrastructure::collect::command::ProcessCollectTxReportCommand>,
        confirm_report_tx: tokio::sync::mpsc::Sender<crate::infrastructure::collect::command::ProcessCollectTxConfirmReportCommand>,
    ) -> Self {
        let semaphore_size = config.semaphore_size;
        Self {
            pool,
            config,
            running: DashSet::new(),
            semaphore: Arc::new(Semaphore::new(semaphore_size)),
            tx_tx,
            report_tx,
            confirm_report_tx,
        }
    }

    /// 处理推进意图
    pub async fn handle_intent(&self, intent: CollectIntent) -> Result<(), anyhow::Error> {
        let trade_no = match &intent {
            CollectIntent::BuildTx(trade_no) => trade_no.clone(),
            CollectIntent::Broadcast(trade_no) => trade_no.clone(),
            CollectIntent::Confirm(trade_no) => trade_no.clone(),
            CollectIntent::Ack(trade_no) => trade_no.clone(),
        };

        info!(?intent, trade_no = %trade_no, "Received collect intent");

        // 1. 检查是否正在执行
        if !self.running.insert(trade_no.clone()) {
            debug!(trade_no = %trade_no, "Trade no already in running set, skipping");
            return Ok(());
        }

        // 2. 获取信号量许可
        let _permit = self.semaphore.acquire().await?;

        // 3. DB状态二次校验
        let should_proceed = match self.check_db_state(&intent).await {
            Ok(should) => should,
            Err(e) => {
                warn!(trade_no = %trade_no, error = %e, "DB state check failed");
                self.running.remove(&trade_no);
                return Err(e);
            }
        };

        if !should_proceed {
            info!(trade_no = %trade_no, "DB state not match expected, skipping");
            self.running.remove(&trade_no);
            return Ok(());
        }

        // 4. 决策投递（实际发送到原有channel）
        match intent {
            CollectIntent::BuildTx(trade_no) | CollectIntent::Broadcast(trade_no) => {
                info!(trade_no = %trade_no, "Sending Tx command to original channel");
                self.tx_tx.send(
                    crate::infrastructure::collect::command::ProcessCollectTxCommand::Tx(trade_no.clone())
                ).await?;
            },
            CollectIntent::Confirm(trade_no) => {
                info!(trade_no = %trade_no, "Sending Confirm command to original channel");
                self.confirm_report_tx.send(
                    crate::infrastructure::collect::command::ProcessCollectTxConfirmReportCommand::Tx(trade_no.clone())
                ).await?;
            },
            CollectIntent::Ack(trade_no) => {
                info!(trade_no = %trade_no, "Sending Report command to original channel for ACK");
                self.report_tx.send(
                    crate::infrastructure::collect::command::ProcessCollectTxReportCommand::Tx(trade_no.clone())
                ).await?;
            },
        }

        // 5. 从正在执行集合中移除
        self.running.remove(&trade_no);

        Ok(())
    }

    /// 检查DB状态是否符合预期
    async fn check_db_state(&self, intent: &CollectIntent) -> Result<bool, anyhow::Error> {
        let trade_no = match intent {
            CollectIntent::BuildTx(trade_no) => trade_no,
            CollectIntent::Broadcast(trade_no) => trade_no,
            CollectIntent::Confirm(trade_no) => trade_no,
            CollectIntent::Ack(trade_no) => trade_no,
        };

        // 查询最新的DB状态
        let collect = ApiCollectRepo::get_api_collect_by_trade_no(&self.pool, trade_no)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get api collect by trade_no: {}", e))?;

        // 根据意图检查状态是否符合预期
        let expected = match intent {
            CollectIntent::BuildTx(_) => {
                // INIT状态才需要BuildTx
                ApiCollectStatus::Init
            },
            CollectIntent::Broadcast(_) => {
                // SENDING状态才需要Broadcast
                ApiCollectStatus::SendingTx
            },
            CollectIntent::Confirm(_) => {
                // SENDING状态才需要Confirm
                ApiCollectStatus::SendingTxReport
            },
            CollectIntent::Ack(_) => {
                // SUCCESS或FAILURE状态才需要Ack
                // 同时检查tx_res_ack_sent_at是否为NULL
                if !(collect.status == ApiCollectStatus::Success || collect.status == ApiCollectStatus::Failure) {
                    return Ok(false);
                }
                // 已经发送过ACK，不需要再次发送
                if collect.tx_res_ack_sent_at.is_some() {
                    return Ok(false);
                }
                return Ok(true);
            },
        };

        // 检查状态是否匹配
        Ok(collect.status == expected)
    }


}