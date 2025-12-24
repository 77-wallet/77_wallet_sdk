// bootstrap.rs
use wallet_database::repositories::api_wallet::expand_batch::ExpandBatchRepo;

use crate::{
    error::service::ServiceError,
    infrastructure::expand_address::{
        actor::ExpandActorMsg, facade::ExpandAddressFacade, service::ExpandService,
    },
};

pub(crate) struct ExpandBootstrap;

impl ExpandBootstrap {
    /// 程序启动时调用：恢复所有未完成的 expand 批次
    pub async fn recover_unfinished_expand_items() -> Result<(), ServiceError> {
        tracing::info!("开始 recover 未完成的 expand items");

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

            let actor = ExpandAddressFacade::get_or_create_actor(&b.uid, &b.chain_code).await?;
            actor.send(ExpandActorMsg::RecoverTask { reply: None }).await?;

            tracing::info!(
                uid=%b.uid,
                chain=%b.chain_code,
                "已发送 RecoverTask"
            );
        }

        Ok(())
    }

    /// 恢复未完成的expand_address_complete操作
    /// 程序启动时调用，检查所有AwmCmdAddrExpand任务，找出那些地址已全部初始化但未发送完成通知的任务
    pub async fn recover_unfinished_expand_complete() -> Result<(), ServiceError> {
        tracing::info!("开始恢复未完成的地址扩展完成操作");

        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        let done = ExpandBatchRepo::get_all_done_but_not_notified(pool.clone()).await?;

        for batch in done {
            let actor =
                ExpandAddressFacade::get_or_create_actor(&batch.uid, &batch.chain_code).await?;
            actor.send(ExpandActorMsg::Schedule).await?;
        }
        Ok(())
    }
}
