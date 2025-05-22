use crate::infrastructure::task_queue::task_manager::TaskManager;

use super::RunningTasks;

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};
use wallet_database::entities::task_queue::TaskQueueEntity;

type Priority = u8;
type PriorityTaskQueue = Arc<tokio::sync::Mutex<BTreeMap<Priority, VecDeque<TaskQueueEntity>>>>;

#[derive(Debug, Clone)]
pub(super) struct Dispatcher {
    task_queues: Arc<tokio::sync::Mutex<BTreeMap<u8, VecDeque<TaskQueueEntity>>>>,
    semaphore: Arc<Semaphore>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self {
            task_queues: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
            semaphore: Arc::new(Semaphore::new(50)), // 最大并发数可调整
        }
    }

    /// 任务调度器 `task_dispatcher`
    ///
    /// 该调度器设计用于多优先级任务队列的异步调度与执行，
    /// 结合了无界队列用于接收任务和有限队列控制并发，
    /// 同时实现优先级公平轮询机制，防止高优先级任务饿死低优先级任务。
    ///
    /// ## 结构与流程
    ///
    /// ```text
    ///  上层模块 / 外部调用
    ///            │
    ///            ▼
    ///  UnboundedSender<(priority, Vec<Task>)>  (无界通道)
    ///            │
    ///   ┌────────┴─────────────────┐
    ///   │ 任务接收处理任务            │
    ///   │ (将任务按优先级放入内部队列)  │
    ///   ▼                      internal priority queues (BTreeMap<Priority, VecDeque<Task>>)
    ///                          (用 Mutex 保护)
    ///            │
    ///  调度器轮询任务队列，
    ///  按优先级公平轮询方式从每个队列取有限任务（最大 N 个）
    ///            │
    ///  有界信号量(Semaphore)限制最大并发任务数
    ///            │
    ///  tokio::spawn 启动任务异步执行，
    ///  任务执行结束后释放信号量许可
    /// ```
    ///
    /// ## 主要组件
    ///
    /// - `external_tx`：
    ///   外部任务发送通道（无界），发送 `(优先级, Vec<Task>)`，保证外部调用不阻塞。
    ///
    /// - `task_queues`：
    ///   内部优先级任务队列，
    ///   使用 `BTreeMap<u8, VecDeque<TaskQueueEntity>>` 以优先级排序，
    ///   通过异步 Mutex 保护并发安全。
    ///
    /// - `semaphore`：
    ///   限制最大并发任务数（例如 50），防止过载。
    ///
    /// - 任务接收任务（任务接收循环）：
    ///   持续监听 `external_rx`，
    ///   将外部传来的任务按优先级分类放入对应队列。
    ///
    /// - 调度器主循环：
    ///   轮询所有优先级队列，
    ///   每个队列最多取 `MAX_TASKS_PER_ROUND` 任务，
    ///   聚合后依次启动异步任务执行，
    ///   启动前通过 `running_tasks` 集合避免重复运行相同任务。
    ///
    /// - 异步任务执行：
    ///   通过 `TaskManager::process_single_task` 处理单个任务，
    ///   并确保执行完成后释放信号量许可，
    ///   控制并发数，保持系统稳定。
    ///
    ///
    /// ## 公平调度说明
    ///
    /// 调度器每轮从所有优先级队列轮流取任务，
    /// 避免高优先级任务一直占用资源导致低优先级任务饥饿，
    /// 保证各优先级任务均能得到合理执行机会。
    ///
    /// ## 性能优化
    ///
    /// - 空转时延时 200ms 避免忙等待。
    /// - 每启动一个任务后延时 100ms，
    ///   防止瞬时过多并发启动导致资源冲击。
    ///
    pub(super) fn task_dispatcher(
        &self,
        running_tasks: RunningTasks,
    ) -> tokio::sync::mpsc::UnboundedSender<(u8, Vec<TaskQueueEntity>)> {
        // 外部接口按 (priority, Vec<Task>) 发送
        let (external_tx, mut external_rx) =
            tokio::sync::mpsc::unbounded_channel::<(u8, Vec<TaskQueueEntity>)>();

        // 每个优先级的任务队列（u8 优先级 → VecDeque）
        let task_queues = Arc::clone(&self.task_queues);

        // 收集任务：不断将接收到的任务填入对应优先级队列
        tokio::spawn(async move {
            while let Some((priority, task_list)) = external_rx.recv().await {
                let mut queues = task_queues.lock().await;
                let queue = queues.entry(priority).or_default();
                for task in task_list {
                    queue.push_back(task);
                }
            }
        });

        // 主调度器，轮询各个优先级队列进行公平分发
        let task_queues = Arc::clone(&self.task_queues);
        let semaphore = Arc::clone(&self.semaphore);

        tokio::spawn(async move {
            const MAX_TASKS_PER_ROUND: usize = 3;

            loop {
                let mut round: Vec<TaskQueueEntity> = vec![];

                {
                    let mut queues = task_queues.lock().await;
                    for (_priority, queue) in queues.iter_mut() {
                        let take_count = std::cmp::min(MAX_TASKS_PER_ROUND, queue.len());
                        for _ in 0..take_count {
                            if let Some(task) = queue.pop_front() {
                                round.push(task);
                            }
                        }
                    }
                }

                // 空转时适当 sleep，避免 busy loop
                if round.is_empty() {
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    continue;
                }

                // 实际执行任务
                for task in round {
                    let task_id = task.id.clone();
                    if running_tasks.insert(task_id.clone()) {
                        let semaphore = Arc::clone(&semaphore);
                        let running_tasks = Arc::clone(&running_tasks);

                        let permit = semaphore.acquire_owned().await.unwrap();
                        tokio::spawn(async move {
                            TaskManager::process_single_task(task, running_tasks).await;
                            drop(permit);
                        });

                        // 可选小延迟避免瞬时并发冲击
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    }
                }
            }
        });
        external_tx
    }
}
