use std::sync::Arc;

use once_cell::sync::Lazy;
use wallet_database::{
    entities::expand_batch_item::ExpandItemStatus,
    repositories::api_wallet::expand_batch_item::ExpandBatchItemRepo,
};

use crate::{
    error::{service::ServiceError, system::SystemError},
    infrastructure::expand_address::{
        actor::ExpandActorMsg, facade::ExpandAddressFacade, service::ExpandService,
    },
};

use tokio::sync::{Semaphore, mpsc};

#[derive(Debug)]
pub(crate) enum ExpandJob {
    Create { uid: String, chain: String, batch_id: String, indices: Vec<i32> },
    Init { uid: String, chain: String, batch_id: String, indices: Vec<i32> },
}

pub(crate) struct ExpandWorkerPool {
    sem: Arc<Semaphore>,
    pub(crate) tx: mpsc::Sender<ExpandJob>,
}

pub(crate) static WORKER_POOL: Lazy<ExpandWorkerPool> = Lazy::new(|| {
    let (tx, mut rx) = mpsc::channel::<ExpandJob>(1024);
    let sem = Arc::new(Semaphore::new(super::actor::EXPAND_MAX_INFLIGHT));

    let sem_c = sem.clone();
    tokio::spawn(async move {
        while let Some(job) = rx.recv().await {
            let permit = sem_c.clone().acquire_owned().await.unwrap();
            tokio::spawn(async move {
                let _p = permit;
                if let Err(e) = run_expand_job(job).await {
                    tracing::error!("expand worker job failed: {:?}", e);
                }
            });
        }
    });

    ExpandWorkerPool { sem, tx }
});

async fn run_expand_job(job: ExpandJob) -> Result<(), ServiceError> {
    // 等系统 ready（密码缓存、Context 初始化等）
    crate::infrastructure::system_ready::wait_system_ready().await;

    let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
    let (uid, chain, batch_id, indices) = match &job {
        ExpandJob::Create { uid, chain, batch_id, indices } => {
            (uid.clone(), chain.clone(), batch_id.clone(), indices.clone())
        }
        ExpandJob::Init { uid, chain, batch_id, indices } => {
            (uid.clone(), chain.clone(), batch_id.clone(), indices.clone())
        }
    };

    let result = match &job {
        ExpandJob::Create { uid, chain, batch_id, indices } => {
            tracing::info!(uid=%uid, chain=%chain, batch_id=%batch_id, "开始执行地址创建任务");
            ExpandService::create_account(&uid, &chain, &indices, &batch_id).await
        }
        ExpandJob::Init { uid, chain, batch_id, indices } => {
            tracing::info!(uid=%uid, chain=%chain, batch_id=%batch_id, "开始执行地址初始化任务");
            ExpandService::init_account(&uid, &chain, &indices, &batch_id).await
        }
    };

    match result {
        Ok(_) => {
            match job {
                ExpandJob::Create { .. } => {
                    // Create 成功 → Initing
                    ExpandBatchItemRepo::mark_items_status_from(
                        pool,
                        &batch_id,
                        &indices,
                        ExpandItemStatus::Creating,
                        ExpandItemStatus::Initing,
                    )
                    .await?;

                    // 通知 actor 索引已创建
                    ExpandAddressFacade::submit_account_created(&uid, &chain, indices).await?;
                }
                ExpandJob::Init { .. } => {}
            }
        }
        Err(e) => {
            if matches!(e, ServiceError::System(SystemError::SystemNotReady)) {
                tracing::warn!(
                    uid=%uid,
                    chain=%chain,
                    batch_id=%batch_id,
                    error=?e,
                    "expand job skipped: system not ready, rollback to Pending"
                );

                match job {
                    ExpandJob::Create { .. } => {
                        ExpandBatchItemRepo::rollback_status(
                            pool,
                            &batch_id,
                            &indices,
                            ExpandItemStatus::Creating,
                            ExpandItemStatus::Pending,
                        )
                        .await?;
                    }
                    ExpandJob::Init { .. } => {
                        ExpandBatchItemRepo::rollback_status(
                            pool,
                            &batch_id,
                            &indices,
                            ExpandItemStatus::Initing,
                            ExpandItemStatus::Pending,
                        )
                        .await?;
                    }
                }

                // 通知 actor 之后再调度
                let actor = ExpandAddressFacade::get_or_create_actor(&uid, &chain).await?;
                actor.send(ExpandActorMsg::Schedule).await?;
                return Ok(());
            }
            match job {
                ExpandJob::Create { .. } => {
                    // Create 失败 → Failed
                    ExpandBatchItemRepo::mark_items_status_from(
                        pool,
                        &batch_id,
                        &indices,
                        ExpandItemStatus::Creating,
                        ExpandItemStatus::Failed,
                    )
                    .await?;
                }
                ExpandJob::Init { .. } => {
                    // Init 失败 → Failed
                    ExpandBatchItemRepo::mark_items_status_from(
                        pool,
                        &batch_id,
                        &indices,
                        ExpandItemStatus::Initing,
                        ExpandItemStatus::Failed,
                    )
                    .await?;
                }
            }
            return Err(e);
        }
    }

    // 跑完后通知 Actor 重新 schedule
    let actor = ExpandAddressFacade::get_or_create_actor(&uid, &chain).await?;
    actor.send(ExpandActorMsg::Schedule).await?;
    Ok(())
}
