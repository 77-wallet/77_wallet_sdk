// bootstrap.rs
use wallet_database::repositories::api_wallet::expand_batch::ExpandBatchRepo;

use crate::{
    error::service::ServiceError,
    infrastructure::expand_address::{facade::ExpandAddressFacade, service::ExpandService},
};

pub(crate) struct ExpandBootstrap;

impl ExpandBootstrap {
    /// 程序启动时调用：恢复所有未完成的 expand 批次的 actor
    pub async fn bootstrap_unfinished_expand_actors() -> Result<(), ServiceError> {
        tracing::info!("开始 bootstrap 未完成的 expand actors");

        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        let batches = ExpandBatchRepo::get_unfinished_batches(pool.clone()).await?;
        tracing::info!("发现未完成批次数量: {}", batches.len());

        let mut seen = std::collections::HashSet::new();

        for b in batches {
            let key = (b.uid.clone(), b.chain_code.clone());
            // 同一个 uid+chain 只需要 recover 一次
            if !seen.insert(key.clone()) {
                continue;
            }

            ExpandAddressFacade::get_or_create_actor(&b.uid, &b.chain_code).await?;
        }

        Ok(())
    }

    /// 恢复未完成的扩容成功操作
    /// 程序启动时调用，检查所有AwmCmdAddrExpand任务，找出那些地址已全部初始化但未发送完成通知的任务
    pub async fn recover_unnotified_expand_batches() -> Result<(), ServiceError> {
        tracing::info!("开始恢复未完成的地址扩展完成操作");

        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        let done = ExpandBatchRepo::get_all_done_but_not_notified(pool.clone()).await?;

        for batch in done {
            ExpandService::expand_complete(&batch.uid, &batch.batch_id).await?;
            ExpandBatchRepo::mark_as_notified(pool.clone(), &batch.batch_id).await?;
        }
        Ok(())
    }
}
