// worker.rs
// 🔴 核心设计原则（**必须严格遵守，否则将导致不可恢复的数据破坏**）
// 🔴 1. Worker/Executor 只执行任务，不参与状态管理
// 🔴 2. 状态管理由 Scanner 负责，基于 DB 事实
// 🔴 3. ExecOutcome 只影响 Worker 内部日志，不允许直接修改 Item / Batch 状态
// 🔴 4. 禁止在 Worker 中根据 ExecOutcome 直接修改 DB 状态
// 🔴 5. 禁止引入 wait_system_ready 作为全局门闩，仅在 Create 任务中使用
// 🔴 6. Worker 是"哑执行器"，只打日志，不重试，只上报执行完成事件，不参与状态决策，不修改状态
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::FutureExt;
use once_cell::sync::Lazy;

use crate::{error::{service::ServiceError, system::SystemError}, infrastructure::expand_address::{event::ExpandEvent, executor::ExpandExecutor, scanner::{ExpandDispatchKey, ExpandJobResult}},
};
use wallet_database::repositories::api_wallet::expand_batch::ExpandBatchRepo;

use tokio::sync::{Semaphore, mpsc};
use std::sync::Arc;

/// 最大同时运行的Create任务数量
pub(crate) const CREATE_MAX_CONCURRENCY: usize = 10;

/// 最大同时运行的Init任务数量
pub(crate) const INIT_MAX_CONCURRENCY: usize = 3;

// ⚠️ IMPORTANT:
// Worker MUST NOT modify expand_batch / expand_batch_item status.
// Worker is only allowed to write fact fields (e.g. expand_complete_at).
// All state transitions are owned by ExpandScanner.

// 任务类型枚举，用于执行层区分任务类型，避免依赖ExpandJob
#[derive(Debug, Clone, Copy)]
enum JobKind {
    Create,
    Init,
    Notify,
}

// Job ID generator
static JOB_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

// Semaphores for Create/Init concurrency control
static CREATE_SEMAPHORE: Lazy<Arc<Semaphore>> = Lazy::new(|| Arc::new(Semaphore::new(CREATE_MAX_CONCURRENCY)));
static INIT_SEMAPHORE: Lazy<Arc<Semaphore>> = Lazy::new(|| Arc::new(Semaphore::new(INIT_MAX_CONCURRENCY)));

// 活跃任务计数，用于监控Worker负载
static EXPAND_WORKER_INFLIGHT: AtomicUsize = AtomicUsize::new(0);

// 全局信号量，限制最大并发任务数
static MAX_CONCURRENT: usize = 8;
static EXPAND_SEMAPHORE: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(MAX_CONCURRENT));

#[derive(Debug, Clone)]
pub(crate) enum ExpandJob {
    Create {
        job_id: String,
        uid: String,
        chain: String,
        batch_id: String,
        indices: Vec<i32>,
        dispatch_key: ExpandDispatchKey,
        result_tx: tokio::sync::mpsc::UnboundedSender<ExpandJobResult>,
    },
    Init {
        job_id: String,
        uid: String,
        chain: String,
        batch_id: String,
        indices: Vec<i32>,
        dispatch_key: ExpandDispatchKey,
        result_tx: tokio::sync::mpsc::UnboundedSender<ExpandJobResult>,
    },
    Notify {
        job_id: String,
        uid: String,
        chain: String,
        batch_id: String,
        dispatch_key: ExpandDispatchKey,
        result_tx: tokio::sync::mpsc::UnboundedSender<ExpandJobResult>,
    },
}

impl ExpandJob {
    pub fn generate_job_id() -> String {
        JOB_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed).to_string()
    }

    pub fn new_create(
        uid: String,
        chain: String,
        batch_id: String,
        indices: Vec<i32>,
        dispatch_key: ExpandDispatchKey,
        result_tx: tokio::sync::mpsc::UnboundedSender<ExpandJobResult>,
    ) -> Self {
        Self::Create {
            job_id: Self::generate_job_id(),
            uid,
            chain,
            batch_id,
            indices,
            dispatch_key,
            result_tx,
        }
    }

    pub fn new_init(
        uid: String,
        chain: String,
        batch_id: String,
        indices: Vec<i32>,
        dispatch_key: ExpandDispatchKey,
        result_tx: tokio::sync::mpsc::UnboundedSender<ExpandJobResult>,
    ) -> Self {
        Self::Init {
            job_id: Self::generate_job_id(),
            uid,
            chain,
            batch_id,
            indices,
            dispatch_key,
            result_tx,
        }
    }

    pub fn new_notify(
        uid: String,
        chain: String,
        batch_id: String,
        dispatch_key: ExpandDispatchKey,
        result_tx: tokio::sync::mpsc::UnboundedSender<ExpandJobResult>,
    ) -> Self {
        Self::Notify {
            job_id: Self::generate_job_id(),
            uid,
            chain,
            batch_id,
            dispatch_key,
            result_tx,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            ExpandJob::Create { job_id, .. } => job_id,
            ExpandJob::Init { job_id, .. } => job_id,
            ExpandJob::Notify { job_id, .. } => job_id,
        }
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
    pub(crate) tx: mpsc::Sender<ExpandJob>,
}

pub(crate) static WORKER_POOL: Lazy<ExpandWorkerPool> = Lazy::new(|| {
    let (tx, mut rx) = mpsc::channel::<ExpandJob>(1024);

    // 启动单个 dispatcher 任务，负责从通道接收任务并分发
    tokio::spawn(async move {
        tracing::info!("expand dispatcher started");

        while let Some(job) = rx.recv().await {
            tracing::info!(job_id = %job.id(), "DISPATCH: received job");

            // 获取信号量许可，控制并发数
            let permit = match EXPAND_SEMAPHORE.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    tracing::warn!("DISPATCH: semaphore closed, shutting down dispatcher");
                    break;
                }
            };

            // 为每个任务生成新的 tokio 任务
            tokio::spawn(async move {
                // 更新进行中任务计数
                EXPAND_WORKER_INFLIGHT.fetch_add(1, Ordering::Relaxed);

                tracing::info!(
                    job_id = %job.id(),
                    job_type = %job.job_type(),
                    batch_id = %job.batch_id(),
                    "WORKER: starting job"
                );

                // 使用 panic 隔离，确保单个任务 panic 不会影响其他任务
                let result =
                    std::panic::AssertUnwindSafe(handle_job(job))
                        .catch_unwind()
                        .await;

                // 任务完成，更新计数并释放信号量许可
                EXPAND_WORKER_INFLIGHT.fetch_sub(1, Ordering::Relaxed);
                drop(permit);

                // 处理任务执行结果
                match result {
                    Ok(Ok(())) =>
                        tracing::info!("WORKER: completed successfully"),
                    Ok(Err(e)) =>
                        tracing::error!(error = %e, "WORKER: failed"),
                    Err(pe) =>
                        tracing::error!(panic = ?pe, "WORKER: panicked"),
                }
            });
        }

        tracing::info!("expand dispatcher exiting, channel closed");
    });

    ExpandWorkerPool { tx }
});

async fn handle_job(job: ExpandJob) -> Result<(), ServiceError> {
    match job {
        ExpandJob::Create { job_id, uid, chain, batch_id, indices, dispatch_key, result_tx } => {
            let _permit = CREATE_SEMAPHORE.acquire().await.map_err(|e| {
                ServiceError::System(SystemError::Internal(format!(
                    "Failed to acquire CREATE semaphore: {}",
                    e
                )))
            })?;
            let result = run_create(job_id, uid, chain, batch_id, indices).await;

            // Send job result without consuming the result
            if result.is_ok() {
                let _ = result_tx.send(ExpandJobResult::Succeeded { key: dispatch_key });
            } else {
                let _ = result_tx.send(ExpandJobResult::Failed {
                    key: dispatch_key,
                    error: result.as_ref().unwrap_err().to_string(),
                });
            }

            result
        }
        ExpandJob::Init { job_id, uid, chain, batch_id, indices, dispatch_key, result_tx } => {
            let _permit = INIT_SEMAPHORE.acquire().await.map_err(|e| {
                ServiceError::System(SystemError::Internal(format!(
                    "Failed to acquire INIT semaphore: {}",
                    e
                )))
            })?;
            let result = run_init(job_id, uid, chain, batch_id, indices).await;

            // Send job result without consuming the result
            if result.is_ok() {
                let _ = result_tx.send(ExpandJobResult::Succeeded { key: dispatch_key });
            } else {
                let _ = result_tx.send(ExpandJobResult::Failed {
                    key: dispatch_key,
                    error: result.as_ref().unwrap_err().to_string(),
                });
            }

            result
        }
        ExpandJob::Notify { job_id, uid, chain, batch_id, dispatch_key, result_tx } => {
            // Notify任务作为普通job处理，不spawn新任务
            // 这样可以确保inflight计数正确
            let result = run_notify(job_id.clone(), uid, chain, batch_id).await;

            // Send job result without consuming the result
            if result.is_ok() {
                let _ = result_tx.send(ExpandJobResult::Succeeded { key: dispatch_key });
            } else {
                let _ = result_tx.send(ExpandJobResult::Failed {
                    key: dispatch_key,
                    error: result.as_ref().unwrap_err().to_string(),
                });
            }

            if let Err(e) = &result {
                tracing::error!(
                    job_id = %job_id,
                    error = %e,
                    "WORKER: notify job failed"
                );
            }

            result
        }
    }
}

async fn run_create(
    job_id: String,
    uid: String,
    chain: String,
    batch_id: String,
    indices: Vec<i32>,
) -> Result<(), ServiceError> {
    tracing::info!(
        job_id = %job_id,
        uid = %uid,
        chain = %chain,
        batch_id = %batch_id,
        indices_count = indices.len(),
        "WORKER: starting create task"
    );

    // 只有 Create 任务需要等系统 ready（密码缓存、Context 初始化等）
    tracing::info!(job_id = %job_id, "WORKER: waiting system ready");
    let start = std::time::Instant::now();
    crate::infrastructure::system_ready::wait_system_ready().await;
    tracing::info!(
        job_id = %job_id,
        elapsed = ?start.elapsed(),
        "WORKER: system ready passed"
    );

    let executor = ExpandExecutor::new();
    let result = executor.execute_create(&uid, &chain, &indices, &batch_id).await;

    handle_execution_result(&job_id, &batch_id, JobKind::Create, result).await
}

async fn run_init(
    job_id: String,
    uid: String,
    chain: String,
    batch_id: String,
    indices: Vec<i32>,
) -> Result<(), ServiceError> {
    const INIT_CHUNK_SIZE: usize = 40;

    tracing::info!(
        job_id = %job_id,
        uid = %uid,
        chain = %chain,
        batch_id = %batch_id,
        indices_count = indices.len(),
        chunk_size = INIT_CHUNK_SIZE,
        "WORKER: starting init task"
    );

    // 拆分成 40 个一组的 chunks
    let chunks = indices.chunks(INIT_CHUNK_SIZE);
    let chunk_count = chunks.len();

    tracing::info!(
        job_id = %job_id,
        chunk_count = chunk_count,
        "WORKER: split into chunks"
    );

    // 为每个 chunk 创建任务并提交到 INIT_POOL
    let executor = ExpandExecutor::new();
    for (i, chunk) in chunks.enumerate() {
        let chunk_indices = chunk.to_vec();
        tracing::info!(
            job_id = %job_id,
            chunk_index = i,
            chunk_size = chunk_indices.len(),
            "WORKER: submitting chunk to INIT_POOL"
        );

        // 创建 INIT 请求
        let result = executor.execute_init(&uid, &chain, &chunk_indices, &batch_id).await;
        let _ = handle_execution_result(&job_id, &batch_id, JobKind::Init, result).await;
    }
    Ok(())
}

async fn run_notify(
    job_id: String,
    uid: String,
    chain: String,
    batch_id: String,
) -> Result<(), ServiceError> {
    tracing::info!(
        job_id = %job_id,
        uid = %uid,
        chain = %chain,
        batch_id = %batch_id,
        "WORKER: starting notify task"
    );

    let executor = ExpandExecutor::new();
    let result = executor.execute_notify(&uid, &batch_id).await;

    handle_execution_result(&job_id, &batch_id, JobKind::Notify, result).await
}

/// 记录任务执行结果的事实
async fn record_fact(job_id: &str, batch_id: &str, job_kind: JobKind) {
    match job_kind {
        JobKind::Notify => {
            // 对于Notify任务，记录expand_complete_at事实字段
            if let Ok(context) = crate::context::get_context() {
                if let Ok(pool) = context.get_global_sqlite_pool() {
                    // 记录事实：expand_complete已成功执行
                    if let Err(e) = ExpandBatchRepo::update_expand_complete_at_if_null(pool.clone(), batch_id).await {
                        tracing::error!(error = %e, batch_id = %batch_id, "failed to update expand_complete_at");
                    }
                }
            }
        }
        JobKind::Create | JobKind::Init => {
            // 对于Create/Init任务，不直接写local_complete_at事实
            // local_complete_at只由Scanner作为"最终事实修复者"负责
            // 这保证了事实写入的单一责任，便于调试和维护
        }
    }
}

/// 发送HintScan事件通知Scanner检查状态
async fn emit_hint_scan() {
    // 任务成功完成，发送HintScan事件通知Scanner检查状态
    // 只有在数据库事实已形成后发送
    if let Ok(context) = crate::context::get_context() {
        if let Some(event_tx) = context.get_expand_event_tx().await {
            // best-effort hint, ignore failure
            let _ = event_tx.send(ExpandEvent::HintScan).await;
            tracing::info!("sent HintScan event to scanner");
        }
    }
}

async fn handle_execution_result(
    job_id: &str,
    batch_id: &str,
    job_kind: JobKind,
    result: Result<crate::infrastructure::expand_address::executor::ExecOutcome, ServiceError>,
) -> Result<(), ServiceError> {
    match result {
        Ok(exec_outcome) => {
            match exec_outcome {
                crate::infrastructure::expand_address::executor::ExecOutcome::Success => {
                    tracing::info!(
                        job_id = %job_id,
                        "expand worker job completed successfully"
                    );

                    // 1. 记录事实
                    record_fact(job_id, batch_id, job_kind).await;

                    // 2. 仅为Notify任务发送HintScan事件（只有Notify会立即写入DB事实）
                    if let JobKind::Notify = job_kind {
                        emit_hint_scan().await;
                    }
                }
                crate::infrastructure::expand_address::executor::ExecOutcome::Retryable {
                    reason,
                } => {
                    tracing::warn!(
                        job_id = %job_id,
                        reason = ?reason,
                        "expand worker job failed with retryable error, scanner will handle retry"
                    );
                    // 可重试错误，记录日志后继续
                    // 当前设计中，Worker 不会重试，Scanner 会基于 DB 事实重试
                }
                crate::infrastructure::expand_address::executor::ExecOutcome::Fatal { reason } => {
                    tracing::error!(
                        job_id = %job_id,
                        reason = ?reason,
                        "fatal error: no retry possible, scanner will observe DB facts and stop progressing"
                    );
                    // 致命错误，记录日志后继续
                    // 不可重试，Scanner会基于DB事实停止推进该item
                }
            }
        }
        Err(e) => {
            // 系统错误，记录日志
            tracing::error!(
                job_id = %job_id,
                error = %e,
                "expand worker job failed with system error; no retry is performed by worker"
            );
            // 系统错误，返回错误
        }
    }

    Ok(())
}