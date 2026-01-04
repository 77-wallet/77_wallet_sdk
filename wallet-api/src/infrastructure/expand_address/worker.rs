// worker.rs
// 🔴 核心设计原则（**必须严格遵守，否则将导致不可恢复的数据破坏**）
// 🔴 1. Worker/Executor 只执行任务，不参与状态管理
// 🔴 2. 状态管理由 Scanner 负责，基于 DB 事实
// 🔴 3. ExecOutcome 只影响 Worker 内部日志，不允许直接修改 Item / Batch 状态
// 🔴 4. 禁止在 Worker 中根据 ExecOutcome 直接修改 DB 状态
// 🔴 5. 禁止引入 wait_system_ready 作为全局门闩，仅在 Create 任务中使用
// 🔴 6. Worker 是"哑执行器"，只打日志，不重试，不上报结果，不修改状态
use std::sync::{Arc, atomic::AtomicUsize};

use once_cell::sync::Lazy;

use crate::{
    error::service::ServiceError,
    infrastructure::expand_address::{event::ExpandEvent, executor::ExpandExecutor},
};
use wallet_database::repositories::api_wallet::expand_batch::ExpandBatchRepo;

use tokio::sync::{Semaphore, mpsc};

/// 最大同时运行的扩容任务数量
pub(crate) const EXPAND_MAX_INFLIGHT: usize = 64;

/// 最大同时运行的Create任务数量
pub(crate) const CREATE_MAX_CONCURRENCY: usize = 3;

/// 最大同时运行的Init任务数量
pub(crate) const INIT_MAX_CONCURRENCY: usize = 3;

// ⚠️ IMPORTANT:
// Worker MUST NOT modify expand_batch / expand_batch_item status.
// Worker is only allowed to write fact fields (e.g. expand_complete_at).
// All state transitions are owned by ExpandScanner.

// Job ID generator
static JOB_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

// Semaphores for Create/Init concurrency control
static CREATE_SEMAPHORE: Lazy<Arc<Semaphore>> =
    Lazy::new(|| Arc::new(Semaphore::new(CREATE_MAX_CONCURRENCY)));
static INIT_SEMAPHORE: Lazy<Arc<Semaphore>> =
    Lazy::new(|| Arc::new(Semaphore::new(INIT_MAX_CONCURRENCY)));

fn generate_job_id() -> usize {
    JOB_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[derive(Debug, Clone)]
pub(crate) enum ExpandJob {
    Create { uid: String, chain: String, batch_id: String, indices: Vec<i32> },
    Init { uid: String, chain: String, batch_id: String, indices: Vec<i32> },
    Notify { uid: String, chain: String, batch_id: String },
}

impl ExpandJob {
    pub fn id(&self) -> String {
        format!("{}-{}", generate_job_id(), self.job_type())
    }

    pub fn job_type(&self) -> &str {
        match self {
            ExpandJob::Create { .. } => "create",
            ExpandJob::Init { .. } => "init",
            ExpandJob::Notify { .. } => "notify",
        }
    }

    pub fn batch_id(&self) -> &str {
        match self {
            ExpandJob::Create { batch_id, .. } => batch_id,
            ExpandJob::Init { batch_id, .. } => batch_id,
            ExpandJob::Notify { batch_id, .. } => batch_id,
        }
    }
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
            // 提前生成job_id和提取job_type，避免在spawn中借用job
            let job_id = job.id();
            let job_id_clone = job_id.clone(); // 克隆job_id，用于run_expand_job
            let job_type = job.job_type().to_string(); // 转换为String，避免引用
            let batch_id = job.batch_id().to_string();

            tracing::info!(
                job_id = %job_id,
                job_type = %job_type,
                batch_id = %batch_id,
                "WORKER: waiting for permit"
            );

            let permit = sem_c.clone().acquire_owned().await.unwrap();

            tracing::info!(
                job_id = %job_id,
                job_type = %job_type,
                batch_id = %batch_id,
                "WORKER: permit acquired"
            );

            // 克隆job到spawn闭包中，避免生命周期问题
            let cloned_job = job.clone();
            let job_id_spawn = job_id.clone(); // 用于spawn闭包中的日志

            tokio::spawn(async move {
                let _p = permit;

                tracing::info!(
                    job_id = %job_id_spawn,
                    job_type = %job_type,
                    batch_id = %batch_id,
                    "WORKER: job started"
                );

                if let Err(e) = run_expand_job(cloned_job, job_id_clone).await {
                    tracing::error!(job_id = %job_id_spawn, error = %e, "expand worker job failed");
                }

                tracing::info!(
                    job_id = %job_id_spawn,
                    job_type = %job_type,
                    batch_id = %batch_id,
                    "WORKER: job finished, releasing permit"
                );
            });
        }
    });

    ExpandWorkerPool { sem, tx }
});

async fn run_expand_job(job: ExpandJob, job_id: String) -> Result<(), ServiceError> {
    // 创建 Executor 实例，执行具体操作
    let executor = ExpandExecutor::new();

    // 根据job类型获取对应的semaphore permit
    let permit = match &job {
        ExpandJob::Create { .. } => {
            tracing::info!(job_id = %job_id, "WORKER: waiting for CREATE semaphore permit");
            Some(CREATE_SEMAPHORE.clone().acquire_owned().await.unwrap())
        }
        ExpandJob::Init { .. } => {
            tracing::info!(job_id = %job_id, "WORKER: waiting for INIT semaphore permit");
            Some(INIT_SEMAPHORE.clone().acquire_owned().await.unwrap())
        }
        ExpandJob::Notify { .. } => {
            // Notify任务不限制并发
            None
        }
    };

    let result = match &job {
        ExpandJob::Create { uid, chain, batch_id, indices } => {
            tracing::info!(job_id = %job_id, uid=%uid, chain=%chain, batch_id=%batch_id, indices_count=indices.len(), "WORKER: starting create task");
            // 只有 Create 任务需要等系统 ready（密码缓存、Context 初始化等）
            tracing::info!(job_id = %job_id, "WORKER: waiting system ready");
            let start = std::time::Instant::now();
            crate::infrastructure::system_ready::wait_system_ready().await;
            tracing::info!(job_id = %job_id, elapsed = ?start.elapsed(), "WORKER: system ready passed");
            executor.execute_create(&uid, &chain, &indices, &batch_id).await
        }
        ExpandJob::Init { uid, chain, batch_id, indices } => {
            tracing::info!(job_id = %job_id, uid=%uid, chain=%chain, batch_id=%batch_id, indices_count=indices.len(), "WORKER: starting init task");
            executor.execute_init(&uid, &chain, &indices, &batch_id).await
        }
        ExpandJob::Notify { uid, chain, batch_id } => {
            tracing::info!(job_id = %job_id, uid=%uid, chain=%chain, batch_id=%batch_id, "WORKER: starting notify task");
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
                    tracing::info!(job_id = %job_id, "expand worker job completed successfully");

                    // 对于Notify任务，记录expand_complete_at事实字段并推进状态到Notified
                    if let ExpandJob::Notify { batch_id, .. } = job {
                        if let Ok(context) = crate::context::get_context() {
                            if let Ok(pool) = context.get_global_sqlite_pool() {
                                // 记录事实：expand_complete已成功执行
                                if let Err(e) = ExpandBatchRepo::update_expand_complete_at_if_null(
                                    pool.clone(),
                                    &batch_id,
                                )
                                .await
                                {
                                    tracing::error!(error = %e, batch_id = %batch_id, "failed to update expand_complete_at");
                                }

                                // 通知成功后，推进状态到Notified
                                // 这是唯一允许推进到Notified状态的地方
                                if let Err(e) =
                                    ExpandBatchRepo::mark_notified_if_done(pool.clone(), &batch_id)
                                        .await
                                {
                                    tracing::error!(error = %e, batch_id = %batch_id, "failed to mark batch as Notified");
                                }
                            }
                        }
                    } else {
                        // 对于Create/Init任务，检查并标记本地完成事实
                        // ⚠️ Worker不负责"判断是否是最后一个"，只负责"尝试确认本地完成事实"
                        // ⚠️ 成功与否由CAS决定，保证只有一个调用者能成功写入local_complete_at
                        let batch_id = job.batch_id();
                        if let Ok(context) = crate::context::get_context() {
                            if let Ok(pool) = context.get_global_sqlite_pool() {
                                if let Err(e) =
                                    ExpandBatchRepo::mark_local_complete_if_all_items_done(
                                        pool.clone(),
                                        batch_id,
                                    )
                                    .await
                                {
                                    tracing::error!(error = %e, batch_id = %batch_id, "failed to mark local complete if all items done");
                                }
                            }
                        }
                    }

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
                    tracing::warn!(job_id = %job_id, reason = ?reason, "expand worker job failed with retryable error, scanner will handle retry");
                    // 可重试错误，记录日志后继续
                    // 当前设计中，Worker 不会重试，Scanner 会基于 DB 事实重试
                }
                crate::infrastructure::expand_address::executor::ExecOutcome::Fatal { reason } => {
                    tracing::error!(job_id = %job_id, reason = ?reason, "fatal error: no retry possible, scanner will observe DB facts and stop progressing");
                    // 致命错误，记录日志后继续
                    // 不可重试，Scanner会基于DB事实停止推进该item
                }
            }
        }
        Err(e) => {
            // 系统错误，记录日志
            tracing::error!(job_id = %job_id, error = %e, "expand worker job failed with system error; no retry is performed by worker");
            // 系统错误，返回错误
        }
    }

    // permit会在函数结束时自动释放
    tracing::info!(job_id = %job_id, "WORKER: releasing semaphore permit");
    Ok(())
}
