// bootstrap.rs
use std::time::Duration;

use wallet_database::repositories::api_wallet::expand_batch::ExpandBatchRepo;

use crate::{
    error::service::ServiceError,
    infrastructure::expand_address::{
        event::channel, scanner::ExpandScanner, service::ExpandService,
    },
};

pub(crate) struct ExpandBootstrap;

impl ExpandBootstrap {
    /// 恢复未完成的扩容成功操作
    /// 程序启动时调用，检查所有AwmCmdAddrExpand任务，找出那些地址已全部初始化但未发送完成通知的任务
    pub async fn recover_unnotified_expand_batches() -> Result<(), ServiceError> {
        tracing::info!("开始恢复未完成的地址扩展完成操作");

        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        let done = ExpandBatchRepo::get_all_done_but_not_notified(pool.clone()).await?;

        for batch in done {
            ExpandService::expand_complete(&batch.uid, &batch.batch_id).await?;
            ExpandBatchRepo::done_to_notified_if_match(pool.clone(), &batch.batch_id).await?;
        }
        Ok(())
    }

    /// 启动ExpandScanner，作为系统的唯一核心驱动
    /// 每30秒执行一次扫描
    pub async fn start_scanner() -> Result<(), ServiceError> {
        tracing::info!("开始启动ExpandScanner作为唯一核心驱动");

        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        // pool已经是Arc<SqlitePool>类型，不需要再次包装

        // 创建事件通道
        // bounded channel to prevent unbounded memory growth
        // overflow is acceptable because events are only hints
        let (event_tx, event_rx) = channel();

        // 将事件发射器保存到全局上下文中，以便其他组件触发事件
        crate::context::get_context()?.set_expand_event_tx(Some(event_tx)).await;

        // 创建并启动Scanner
        // 扫描间隔：30秒
        // 单轮扫描上限：100个items
        let scanner = ExpandScanner::new(pool, Duration::from_secs(30), 100, Some(event_rx));

        // 在后台启动扫描器
        tokio::spawn(async move {
            scanner.start().await;
        });

        tracing::info!("ExpandScanner已成功启动，支持事件驱动");
        Ok(())
    }
}
