// worker.rs
use std::sync::Arc;

use once_cell::sync::Lazy;

use crate::{
    error::service::ServiceError,
    infrastructure::expand_address::{event::ExpandEvent, executor::ExpandExecutor},
};

use tokio::sync::{Semaphore, mpsc};

/// 最大同时运行的扩容任务数量
pub(crate) const EXPAND_MAX_INFLIGHT: usize = 64;

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
    let sem = Arc::new(Semaphore::new(EXPAND_MAX_INFLIGHT));

    let sem_c = sem.clone();
    tokio::spawn(async move {
        while let Some(job) = rx.recv().await {
            let permit = sem_c.clone().acquire_owned().await.unwrap();
            tokio::spawn(async move {
                let _p = permit;
                if let Err(e) = run_expand_job(job).await {
                    tracing::error!(error = %e, "expand worker job failed");
                }
            });
        }
    });

    ExpandWorkerPool { sem, tx }
});

async fn run_expand_job(job: ExpandJob) -> Result<(), ServiceError> {
    // 等系统 ready（密码缓存、Context 初始化等）
    crate::infrastructure::system_ready::wait_system_ready().await;

    // 创建 Executor 实例，执行具体操作
    let executor = ExpandExecutor::new();

    let result = match &job {
        ExpandJob::Create { uid, chain, batch_id, indices } => {
            tracing::info!(uid=%uid, chain=%chain, batch_id=%batch_id, indices_count=indices.len(), "开始执行地址创建任务");
            executor.execute_create(&uid, &chain, &indices, &batch_id).await
        }
        ExpandJob::Init { uid, chain, batch_id, indices } => {
            tracing::info!(uid=%uid, chain=%chain, batch_id=%batch_id, indices_count=indices.len(), "开始执行地址初始化任务");
            executor.execute_init(&uid, &chain, &indices, &batch_id).await
        }
        ExpandJob::Notify { uid, chain, batch_id } => {
            tracing::info!(uid=%uid, chain=%chain, batch_id=%batch_id, "开始执行地址通知任务");
            executor.execute_notify(&uid, &batch_id).await
        }
    };

    // 🔒 明确的边界声明：
    // 🔒 Worker 只执行任务，不参与状态管理
    // 🔒 状态管理由 Scanner 负责，基于 DB 事实
    // 🔒 ExecOutcome 只影响 Worker 内部是否 retry，不允许直接修改 Item / Batch 状态
    // 🔒 Scanner 的状态推进只能基于 DB 事实，禁止基于 ExecOutcome 直接推进状态
    // 🔒 禁止在 Worker 中根据 ExecOutcome 直接修改 DB 状态
    // 🔒 Worker 只负责执行操作并记录结果，状态流转由 Scanner 基于 DB 事实决定
    match result {
        Ok(exec_outcome) => {
            match exec_outcome {
                crate::infrastructure::expand_address::executor::ExecOutcome::Success => {
                    tracing::info!("expand worker job completed successfully");

                    // 任务成功完成，发送HintScan事件通知Scanner检查状态
                    // 只有在数据库事实已形成后发送
                    if let Ok(context) = crate::context::get_context() {
                        if let Some(event_tx) = context.get_expand_event_tx().await {
                            // best-effort hint, ignore failure
                            let _ = event_tx.send(ExpandEvent::HintScan).await;
                        }
                    }
                }
                crate::infrastructure::expand_address::executor::ExecOutcome::Retryable {
                    reason,
                } => {
                    tracing::warn!(reason = ?reason, "expand worker job failed with retryable error, scanner will handle retry");
                    // 可重试错误，记录日志后继续
                    // 当前设计中，Worker 不会重试，Scanner 会基于 DB 事实重试
                }
                crate::infrastructure::expand_address::executor::ExecOutcome::Fatal { reason } => {
                    tracing::error!(reason = ?reason, "fatal error: no retry possible, scanner will observe DB facts and stop progressing");
                    // 致命错误，记录日志后继续
                    // 不可重试，Scanner会基于DB事实停止推进该item
                }
            }
        }
        Err(e) => {
            // 系统错误，记录日志
            tracing::error!(error = %e, "expand worker job failed with system error; no retry is performed by worker");
            // 系统错误，返回错误
        }
    }

    Ok(())
}
