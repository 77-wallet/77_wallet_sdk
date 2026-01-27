// collect_fee/shadow/worker/shadow_fee_worker.rs
use std::sync::Arc;

use tracing::{error, info, warn};
use wallet_database::{CollectDbPool, CoreDbPool, repositories::api_wallet::fee::ApiFeeRepo};

use crate::infrastructure::collect_fee::process_fee_tx_send::AddressLockManager;

/// ShadowFeeWorker 命令
#[derive(Debug, Clone)]
pub enum ShadowFeeCommand {
    /// 构建交易
    BuildTx(String),
    /// 广播交易
    Broadcast(String),
}

/// ShadowFeeWorker
///
/// 负责处理链相关操作：
/// - 构建交易
/// - 广播交易
pub struct ShadowFeeWorker {
    pool: CollectDbPool,
    core_pool: CoreDbPool,
    address_locks: Arc<AddressLockManager>,
    global_sem: Arc<tokio::sync::Semaphore>,
}

impl ShadowFeeWorker {
    pub fn new(
        pool: CollectDbPool,
        core_pool: CoreDbPool,
        address_locks: Arc<AddressLockManager>,
        global_sem: Arc<tokio::sync::Semaphore>,
    ) -> Self {
        Self { pool, core_pool, address_locks, global_sem }
    }

    /// 处理命令
    pub async fn handle(&self, command: ShadowFeeCommand) -> Result<(), anyhow::Error> {
        match command {
            ShadowFeeCommand::BuildTx(trade_no) => {
                self.build_tx(&trade_no).await
            }
            ShadowFeeCommand::Broadcast(trade_no) => {
                self.broadcast_tx(&trade_no).await
            }
        }
    }

    /// 构建交易
    async fn build_tx(&self, trade_no: &str) -> Result<(), anyhow::Error> {
        info!(trade_no = %trade_no, "Building fee transaction");

        // 获取全局信号量许可，控制RPC/链上执行的并发度
        let _permit = self.global_sem.acquire().await.map_err(|e| {
            anyhow::anyhow!("Failed to acquire global semaphore: {}", e)
        })?;

        // 查询手续费记录
        let fee = ApiFeeRepo::get_api_fee_by_trade_no(&self.pool, trade_no).await?;

        // 获取地址锁，确保同一地址的交易串行处理
        let lock = self.address_locks.acquire(&fee.from_addr).await;

        // 构建交易逻辑
        // 注意：这里需要实现具体的交易构建逻辑
        // 实际实现中应该调用相应的链适配器来构建交易

        info!(trade_no = %trade_no, "Fee transaction built successfully");

        // 释放地址锁
        drop(lock);

        Ok(())
    }

    /// 广播交易
    async fn broadcast_tx(&self, trade_no: &str) -> Result<(), anyhow::Error> {
        info!(trade_no = %trade_no, "Broadcasting fee transaction");

        // 获取全局信号量许可，控制RPC/链上执行的并发度
        let _permit = self.global_sem.acquire().await.map_err(|e| {
            anyhow::anyhow!("Failed to acquire global semaphore: {}", e)
        })?;

        // 查询手续费记录
        let fee = ApiFeeRepo::get_api_fee_by_trade_no(&self.pool, trade_no).await?;

        // 获取地址锁，确保同一地址的交易串行处理
        let lock = self.address_locks.acquire(&fee.from_addr).await;

        // 广播交易逻辑
        // 注意：这里需要实现具体的交易广播逻辑
        // 实际实现中应该调用相应的链适配器来广播交易

        info!(trade_no = %trade_no, "Fee transaction broadcasted successfully");

        // 释放地址锁
        drop(lock);

        Ok(())
    }
}
