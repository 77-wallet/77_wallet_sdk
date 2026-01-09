use std::sync::Arc;

use tokio::sync::Semaphore;

/// 后台任务池，用于管理异步执行的副作用任务
/// 阶段1重构：使用 tokio::spawn + Semaphore 替代 JoinSet
#[derive(Debug)]
pub struct BackgroundTaskPool {
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
}

impl BackgroundTaskPool {
    pub fn new(max_concurrent: usize) -> Self {
        Self { semaphore: Arc::new(Semaphore::new(max_concurrent)), max_concurrent }
    }

    /// 添加任务到后台执行
    pub async fn push<F>(&self, future: F)
    where
        F: Future<Output = Result<(), crate::error::service::ServiceError>> + Send + 'static,
    {
        let semaphore = self.semaphore.clone();

        // 使用 tokio::spawn 执行任务，配合 Semaphore 控制并发
        tokio::spawn(async move {
            // 克隆信号量，用于获取可用许可证数量
            let semaphore_clone = semaphore.clone();

            // 尝试获取信号量，如果超过并发限制则等待
            let _permit = match semaphore.acquire_owned().await {
                Ok(permit) => permit,
                Err(e) => {
                    tracing::error!("Failed to acquire semaphore for background task: {:?}", e);
                    return;
                }
            };

            tracing::debug!(
                "Background task started, permits left: {}",
                semaphore_clone.available_permits()
            );

            // 执行任务
            if let Err(e) = future.await {
                tracing::error!("Background task failed: {:?}", e);
            }

            tracing::debug!(
                "Background task finished, permits left: {}",
                semaphore_clone.available_permits() + 1
            );
        });
    }

    /// 获取当前任务数（正在执行的任务数）
    pub async fn len(&self) -> usize {
        // 正在执行的任务数 = 总容量 - 可用许可证数
        self.max_concurrent - self.semaphore.available_permits()
    }
}
