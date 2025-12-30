// worker.rs
use std::sync::Arc;

use once_cell::sync::Lazy;
use wallet_database::entities::expand_batch_item::ExpandItemStatus;

use crate::{
    error::service::ServiceError,
    infrastructure::expand_address::{
        actor::ExpandActorMsg, facade::ExpandAddressFacade, service::ExpandService,
    },
};

use tokio::sync::{Semaphore, mpsc};

#[derive(Debug)]
pub(crate) enum ExpandJob {
    Create { uid: String, chain: String, batch_id: String, indices: Vec<i32> },
    Init { uid: String, chain: String, batch_id: String, indices: Vec<i32> },
    Notify { uid: String, chain: String, batch_id: String },
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

    let (uid, chain, batch_id) = match &job {
        ExpandJob::Create { uid, chain, batch_id, .. }
        | ExpandJob::Init { uid, chain, batch_id, .. }
        | ExpandJob::Notify { uid, chain, batch_id } => {
            (uid.clone(), chain.clone(), batch_id.clone())
        }
    };
    let actor = ExpandAddressFacade::get_or_create_actor(&uid, &chain).await?;

    let result = match &job {
        ExpandJob::Create { uid, chain, batch_id, indices } => {
            tracing::info!(uid=%uid, chain=%chain, batch_id=%batch_id, "开始执行地址创建任务");
            ExpandService::create_account(&uid, &chain, &indices, &batch_id).await
        }
        ExpandJob::Init { uid, chain, batch_id, indices } => {
            tracing::info!(uid=%uid, chain=%chain, batch_id=%batch_id, "开始执行地址初始化任务");
            ExpandService::init_account(&uid, &chain, &indices, &batch_id).await
        }
        ExpandJob::Notify { uid, chain, batch_id } => {
            tracing::info!(uid=%uid, chain=%chain, batch_id=%batch_id, "开始执行地址通知任务");
            ExpandService::expand_complete(&uid, &batch_id).await
        }
    };

    match result {
        Ok(_) => {
            match job {
                ExpandJob::Create { uid, chain, batch_id, indices } => {
                    // 通知 actor 索引已创建
                    ExpandAddressFacade::submit_account_created(&uid, &chain, indices).await?;
                }
                ExpandJob::Init { .. } => {}
                ExpandJob::Notify { uid, chain, batch_id } => {
                    // 通知 actor 索引已扩容
                    ExpandAddressFacade::submit_address_expanded(&uid, &chain, &batch_id).await?;
                }
            }
        }
        Err(e) => {
            match job {
                ExpandJob::Create { uid, chain, batch_id, indices } => {
                    actor
                        .send(ExpandActorMsg::JobFailed {
                            phase: ExpandItemStatus::Creating,
                            indices,
                            error: format!("{:?}", e),
                        })
                        .await?;
                }
                ExpandJob::Init { uid, chain, batch_id, indices } => {
                    actor
                        .send(ExpandActorMsg::JobFailed {
                            phase: ExpandItemStatus::Initing,
                            indices,
                            error: format!("{:?}", e),
                        })
                        .await?;
                }
                ExpandJob::Notify { .. } => {}
            };

            return Err(e);
        }
    }

    Ok(())
}
