// bootstrap.rs
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use wallet_database::repositories::api_wallet::expand_batch::ExpandBatchRepo;

use crate::{
    error::service::ServiceError,
    infrastructure::expand_address::{
        event::channel, scanner::ExpandScanner, service::ExpandService,
    },
};

pub(crate) struct ExpandBootstrap;

static EXPAND_SCANNER_STARTED: AtomicBool = AtomicBool::new(false);

impl ExpandBootstrap {
    pub async fn start_after_first_wallet_unlock(
        ctx: &'static crate::context::Context,
    ) -> Result<(), ServiceError> {
        if EXPAND_SCANNER_STARTED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            tracing::info!("ExpandScanner already started, skip");
            return Ok(());
        }

        Self::recover_unnotified_expand_batches(ctx).await?;
        Self::start_scanner(ctx).await?;
        Ok(())
    }

    /// 恢复未完成的扩容成功操作
    /// 程序启动时调用，检查所有AwmCmdAddrExpand任务，找出那些地址已全部初始化但未发送完成通知的任务
    pub async fn recover_unnotified_expand_batches(
        ctx: &'static crate::context::Context,
    ) -> Result<(), ServiceError> {
        tracing::info!("开始恢复未完成的地址扩展完成操作");

        let pool = ctx.api_wallet_pool()?;
        let backend = ctx.get_global_backend_api();

        let done = ExpandBatchRepo::get_all_done_but_not_notified(&pool).await?;

        for batch in done {
            ExpandService::expand_complete_with_ctx(
                &batch.uid,
                &batch.batch_id,
                &pool,
                backend.as_ref(),
            )
            .await?;
            ExpandBatchRepo::done_to_notified_if_match(&pool, &batch.batch_id).await?;
        }
        Ok(())
    }

    /// 启动ExpandScanner，作为系统的唯一核心驱动
    /// 每30秒执行一次扫描
    pub async fn start_scanner(ctx: &'static crate::context::Context) -> Result<(), ServiceError> {
        tracing::info!("开始启动ExpandScanner作为唯一核心驱动");

        let pool = ctx.api_wallet_pool()?;
        // pool已经是Arc<SqlitePool>类型，不需要再次包装

        // 创建事件通道
        // bounded channel to prevent unbounded memory growth
        // overflow is acceptable because events are only hints
        let (event_tx, event_rx) = channel();

        // 将事件发射器保存到全局上下文中，以便其他组件触发事件
        ctx.set_expand_event_tx(Some(event_tx)).await;

        // 创建并启动Scanner
        // 扫描间隔：6秒
        // 单轮扫描上限：5000个items
        let scanner = ExpandScanner::new(ctx, pool, Duration::from_secs(6), 20000, Some(event_rx));

        // 在后台启动扫描器
        tokio::spawn(async move {
            scanner.start().await;
        });

        tracing::info!("ExpandScanner已成功启动，支持事件驱动");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::EXPAND_SCANNER_STARTED;
    use std::sync::atomic::Ordering;

    #[test]
    fn expand_scanner_started_flag_is_atomic_and_single_use() {
        EXPAND_SCANNER_STARTED.store(false, Ordering::Release);
        assert!(
            EXPAND_SCANNER_STARTED
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        );
        assert!(
            EXPAND_SCANNER_STARTED
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        );
        EXPAND_SCANNER_STARTED.store(false, Ordering::Release);
    }
}
