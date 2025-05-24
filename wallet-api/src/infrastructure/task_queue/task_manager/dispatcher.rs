use crate::infrastructure::task_queue::task_manager::scheduler::TASK_CATEGORY_LIMIT;
use crate::infrastructure::task_queue::task_manager::TaskManager;
use crate::infrastructure::task_queue::{Task, TaskType};

use super::RunningTasks;

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Semaphore};
use wallet_database::entities::task_queue::TaskQueueEntity;

type Priority = u8;
pub type TaskSender = tokio::sync::mpsc::UnboundedSender<(u8, Vec<TaskQueueEntity>)>;

// static RATE_LIMITERS: Lazy<
//     std::collections::HashMap<TaskType, Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>>,
// > = Lazy::new(|| {
//     use TaskType::*;
//     let mut m = std::collections::HashMap::new();
//     m.insert(
//         BackendApi,
//         Arc::new(RateLimiter::direct(Quota::per_second(
//             NonZeroU32::new(50).unwrap(),
//         ))),
//     );
//     m.insert(
//         Mqtt,
//         Arc::new(RateLimiter::direct(Quota::per_second(
//             NonZeroU32::new(200).unwrap(),
//         ))),
//     );
//     m.insert(
//         Initialization,
//         Arc::new(RateLimiter::direct(Quota::per_second(
//             NonZeroU32::new(10).unwrap(),
//         ))),
//     );
//     m.insert(
//         Common,
//         Arc::new(RateLimiter::direct(Quota::per_second(
//             NonZeroU32::new(10).unwrap(),
//         ))),
//     );
//     m
// });

// fn get_rate_limiter(
//     category: &TaskType,
// ) -> Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>> {
//     RATE_LIMITERS.get(category).cloned().unwrap()
// }

/// 多优先级任务调度器 `Dispatcher`
///
/// 该调度器用于管理异步任务执行，支持任务优先级调度、类型限速、并发控制，并通过无锁架构提升吞吐能力。
///
/// ### 🚀 设计思想
/// 类似于“分层 Reactor + 任务分级执行器”，每个优先级即一个独立的执行单元，
/// 任务间通过 channel 解耦，具备天然的并行性与隔离性。
///
///
/// ## ✨ 核心特性
///
/// - **每个优先级独立通道**：按优先级自动创建 `UnboundedSender`，每个优先级都有自己的消费任务，无需共享任务队列或锁。
/// - **任务类型限速**：支持对不同 `TaskType` 的任务设置并发上限（如 `sync` 每轮最多处理 N 个）。
/// - **运行中去重**：使用 `RunningTasks` 去重，避免同一个任务重复并发执行。
/// - **全局并发控制**：使用 `Semaphore` 限制系统总并发任务数，防止过载。
/// - **低资源占用**：无中心锁、无全局轮询，按需动态创建执行器，空闲时不会占用 CPU。
///
/// ## ⚙ 架构说明
///
/// ```text
///             上层调用
///                 │
///     ┌────────────┴────────────┐
///     │ external_tx.send(...)   │
///     └────────────┬────────────┘
///                  ▼
///        Dispatcher::start_internal_task
///                  │
///     ┌────────────┴────────────┐
///     │ external_rx 接收任务     │
///     │ 按 priority 分配到对应通道 │
///     └────────────┬────────────┘
///                  ▼
///      HashMap<Priority, UnboundedSender<TaskQueueEntity>>
///          （如无则动态创建）
///
///    每个优先级通道绑定一个 Tokio 任务:
///
///         UnboundedReceiver<TaskQueueEntity>
///                     │
///          每轮从通道中取任务执行：
///           - 类型限速（TASK_CATEGORY_LIMIT）
///           - 并发控制（Semaphore）
///           - 运行中任务去重
///           - 每任务延时，避免突发冲击
/// ```
///
/// ## 📌 示例流程（新任务进来时）
///
/// ```text
/// 1. Dispatcher.external_tx.send((priority, Vec<TaskQueueEntity>))
/// 2. Dispatcher 内部 task 接收并遍历 Vec<TaskQueueEntity>
/// 3. 若该优先级通道不存在，则动态创建 UnboundedSender + Receiver + Tokio 任务
/// 4. 每个任务推入通道，由其独立 Tokio task 处理
/// 5. 任务消费时：
///    - 限速检查（同类型任务是否已达上限）
///    - Semaphore 控制并发许可
///    - 使用 RunningTasks 去重
///    - TaskManager::process_single_task 执行
/// ```
///
///
/// ## 🛠 可配置项
///
/// - `TASK_CATEGORY_LIMIT`：限制每类任务并发数（静态配置）
/// - `Semaphore(50)`：全局最大并发任务数（可动态调整）
///
///
/// ## 🚧 注意事项
///
/// - 当前方案假设任务处理过程较长，动态 spawn 开销可以忽略。
/// - 不再存在中心调度器，所有优先级由其自身任务“自治消费”。
/// - 空闲时不会有忙等待，节省 CPU 资源。
///
#[derive(Debug, Clone)]
pub(crate) struct Dispatcher {
    pub(crate) external_tx: TaskSender,
    // task_queues: PriorityTaskQueue,
    // semaphore: Arc<Semaphore>,
}

impl Dispatcher {
    pub fn new(running_tasks: RunningTasks) -> Self {
        let (external_tx, external_rx) = tokio::sync::mpsc::unbounded_channel();

        Self::start_internal_task(external_rx, running_tasks, Arc::new(Semaphore::new(50)));
        tracing::info!("Dispatcher 启动完成，开始监听外部任务输入...");
        Self {
            external_tx, // task_queues: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
                         // semaphore: Arc::new(Semaphore::new(50)), // 最大并发数可调整
        }
    }

    /// 启动内部异步调度器：
    ///
    /// - 动态接收外部任务（按优先级）
    /// - 若该优先级未注册消费通道，则自动创建通道 + 启动 Tokio task 处理器
    /// - 每个优先级通道独立消费任务，避免全局锁或抢占资源
    fn start_internal_task(
        mut external_rx: tokio::sync::mpsc::UnboundedReceiver<(Priority, Vec<TaskQueueEntity>)>,
        running_tasks: RunningTasks,
        semaphore: Arc<Semaphore>,
    ) {
        tokio::spawn(async move {
            let mut priority_senders: HashMap<Priority, mpsc::UnboundedSender<TaskQueueEntity>> =
                HashMap::new();
            let category_limit = TASK_CATEGORY_LIMIT
                .iter()
                .cloned()
                .collect::<HashMap<_, _>>();

            while let Some((priority, tasks)) = external_rx.recv().await {
                tracing::info!("收到 {} 个任务，优先级 = {}", tasks.len(), priority,);
                let sender = priority_senders.entry(priority).or_insert_with(|| {
                    tracing::info!("创建新的优先级 {} 通道并启动任务消费器", priority);
                    Self::create_priority_channel_task(
                        priority,
                        running_tasks.clone(),
                        semaphore.clone(),
                        category_limit.clone(),
                    )
                });

                for task in tasks {
                    let _ = sender.send(task);
                }
            }
            tracing::warn!("Dispatcher 的 external_rx 关闭，任务监听结束");
        });
    }

    /// 为某个优先级动态创建通道及绑定异步消费任务
    ///
    /// 返回该优先级的 `UnboundedSender<TaskQueueEntity>`，用于后续投递任务

    fn create_priority_channel_task(
        priority: u8,
        running_tasks: RunningTasks,
        semaphore: Arc<Semaphore>,
        category_limit: HashMap<TaskType, usize>,
    ) -> mpsc::UnboundedSender<TaskQueueEntity> {
        let (tx, mut rx) = mpsc::unbounded_channel::<TaskQueueEntity>();
        // let tx_c = tx.clone(); // 不需要重排但保留 clone 用于其他情况
        tokio::spawn(async move {
            let category_counter = Arc::new(Mutex::new(HashMap::<TaskType, usize>::new()));

            while let Some(task_entity) = rx.recv().await {
                let category_counter = category_counter.clone();
                let running_tasks = running_tasks.clone();
                let semaphore = semaphore.clone();
                let category_limit = category_limit.clone();
                tokio::spawn(async move {
                    Self::handle_task_entity(
                        priority,
                        task_entity,
                        category_counter,
                        &category_limit,
                        running_tasks,
                        semaphore,
                    )
                    .await;
                });
            }
            tracing::warn!("优先级通道消费者退出，该通道已关闭");
        });
        tx
    }

    /// 执行任务处理逻辑，包含：
    /// - 任务类型限速
    /// - 并发控制（Semaphore）
    /// - 去重（RunningTasks）
    /// - 任务调度（spawn）
    async fn handle_task_entity(
        priority: u8,
        task_entity: TaskQueueEntity,
        category_counter: Arc<Mutex<HashMap<TaskType, usize>>>,
        category_limit: &HashMap<TaskType, usize>,
        running_tasks: RunningTasks,
        semaphore: Arc<Semaphore>,
    ) {
        let task_id = &task_entity.id;

        let task: Task = match (&task_entity).try_into() {
            Ok(t) => t,
            Err(_) => {
                tracing::warn!("任务解析失败，跳过：id = {:?}", task_id);
                return;
            } // 转换失败直接跳过
        };

        // === 限速逻辑 ===
        let category = task.get_type();

        // let limiter = get_rate_limiter(&category);

        // limiter.until_ready().await;

        let limit = *category_limit.get(&category).unwrap_or(&1);

        // === 等待类型限速窗口 ===
        loop {
            let mut counter = category_counter.lock().await;
            let count = counter.entry(category.clone()).or_insert(0);
            if *count < limit {
                *count += 1;
                tracing::info!(
                    "任务类型 {:?} 优先级 {} 未达到限速上限 ({}/{}), 待执行：{}",
                    category,
                    priority,
                    *count,
                    limit,
                    task_id
                );
                break;
            } else {
                tracing::info!(
                    "任务类型 {:?} 优先级 {} 达到限速上限 ({}/{}), 等待中：{}",
                    category,
                    priority,
                    *count,
                    limit,
                    task_id
                );
            }
            drop(counter); // 释放锁，避免死锁
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }

        // === 去重逻辑 + 并发控制 ===
        let task_id = task_entity.id.clone();
        if running_tasks.insert(task_id.clone()) {
            tracing::info!(
                "准备执行任务 {}，类型 = {:?}，优先级 = {}, 当前并发 = {}",
                task_id,
                category,
                priority,
                running_tasks.len()
            );

            // 如果成功插入，说明之前没有该任务，开始处理
            let permit = semaphore.acquire_owned().await.unwrap();
            let running_tasks_inner = running_tasks.clone();
            let category_counter = category_counter.clone();

            tokio::spawn(async move {
                tracing::info!("开始执行任务 {}", task_id);
                TaskManager::process_single_task(task_entity, running_tasks_inner).await;
                let mut counter = category_counter.lock().await;
                if let Some(count) = counter.get_mut(&category) {
                    *count = count.saturating_sub(1);
                    tracing::info!(?category, current = *count, "任务计数 -1");
                }
                drop(permit); // 释放信号量
                tracing::info!("任务 {} 执行完成", task_id);
            });

            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        } else {
            tracing::info!("任务 {} 已在运行中，跳过重复执行", task_id);
        }
    }
}

// /// 任务调度器 `task_dispatcher`
// ///
// /// 该调度器设计用于多优先级任务队列的异步调度与执行，
// /// 结合了无界队列用于接收任务和有限队列控制并发，
// /// 同时实现优先级公平轮询机制，防止高优先级任务饿死低优先级任务。
// ///
// /// ## 结构与流程
// ///
// /// ```text
// ///  上层模块 / 外部调用
// ///            │
// ///            ▼
// ///  UnboundedSender<(priority, Vec<Task>)>  (无界通道)
// ///            │
// ///   ┌────────┴─────────────────┐
// ///   │ 任务接收处理任务            │
// ///   │ (将任务按优先级放入内部队列)  │
// ///   ▼                      internal priority queues (BTreeMap<Priority, VecDeque<Task>>)
// ///                          (用 Mutex 保护)
// ///            │
// ///  调度器轮询任务队列，
// ///  按优先级公平轮询方式从每个队列取有限任务（最大 N 个）
// ///            │
// ///  有界信号量(Semaphore)限制最大并发任务数
// ///            │
// ///  tokio::spawn 启动任务异步执行，
// ///  任务执行结束后释放信号量许可
// /// ```
// ///
// /// ## 主要组件
// ///
// /// - `external_tx`：
// ///   外部任务发送通道（无界），发送 `(优先级, Vec<Task>)`，保证外部调用不阻塞。
// ///
// /// - `task_queues`：
// ///   内部优先级任务队列，
// ///   使用 `BTreeMap<u8, VecDeque<TaskQueueEntity>>` 以优先级排序，
// ///   通过异步 Mutex 保护并发安全。
// ///
// /// - `semaphore`：
// ///   限制最大并发任务数（例如 50），防止过载。
// ///
// /// - 任务接收任务（任务接收循环）：
// ///   持续监听 `external_rx`，
// ///   将外部传来的任务按优先级分类放入对应队列。
// ///
// /// - 调度器主循环：
// ///   轮询所有优先级队列，
// ///   每个队列最多取 `MAX_TASKS_PER_ROUND` 任务，
// ///   聚合后依次启动异步任务执行，
// ///   启动前通过 `running_tasks` 集合避免重复运行相同任务。
// ///
// /// - 异步任务执行：
// ///   通过 `TaskManager::process_single_task` 处理单个任务，
// ///   并确保执行完成后释放信号量许可，
// ///   控制并发数，保持系统稳定。
// ///
// ///
// /// ## 公平调度说明
// ///
// /// 调度器每轮从所有优先级队列轮流取任务，
// /// 避免高优先级任务一直占用资源导致低优先级任务饥饿，
// /// 保证各优先级任务均能得到合理执行机会。
// ///
// /// ## 性能优化
// ///
// /// - 空转时延时 200ms 避免忙等待。
// /// - 每启动一个任务后延时 100ms，
// ///   防止瞬时过多并发启动导致资源冲击。
// ///
// pub(super) fn task_dispatcher(
//     &self,
//     running_tasks: RunningTasks,
// ) -> tokio::sync::mpsc::UnboundedSender<(u8, Vec<TaskQueueEntity>)> {
//     // 外部接口按 (priority, Vec<Task>) 发送
//     let (external_tx, mut external_rx) =
//         tokio::sync::mpsc::unbounded_channel::<(u8, Vec<TaskQueueEntity>)>();

//     // 每个优先级的任务队列（u8 优先级 → VecDeque）
//     let task_queues = Arc::clone(&self.task_queues);

//     // 收集任务：不断将接收到的任务填入对应优先级队列
//     tokio::spawn(async move {
//         while let Some((priority, task_list)) = external_rx.recv().await {
//             let mut queues = task_queues.lock().await;
//             let queue = queues.entry(priority).or_default();
//             for task in task_list {
//                 queue.push_back(task);
//             }
//         }
//     });

//     // 主调度器，轮询各个优先级队列进行公平分发
//     let task_queues = Arc::clone(&self.task_queues);
//     let semaphore = Arc::clone(&self.semaphore);

//     tokio::spawn(async move {
//         let category_limit: HashMap<TaskType, usize> =
//             TASK_CATEGORY_LIMIT.iter().cloned().collect();

//         loop {
//             let mut round: Vec<TaskQueueEntity> = vec![];
//             let mut category_counter: HashMap<TaskType, usize> = HashMap::new();
//             {
//                 let mut queues = task_queues.lock().await;
//                 for (_priority, queue) in queues.iter_mut() {
//                     let mut i = 0;
//                     while i < queue.len() {
//                         if let Some(task) = queue.get(i) {
//                             let task: Task = task.try_into().unwrap();
//                             let category = task.get_type();
//                             let limit = *category_limit.get(&category).unwrap_or(&1);
//                             let count = category_counter.entry(category).or_insert(0);

//                             if *count < limit {
//                                 let task = queue.remove(i).unwrap();
//                                 round.push(task);
//                                 *count += 1;
//                             } else {
//                                 i += 1;
//                             }
//                         } else {
//                             break;
//                         }
//                     }
//                 }
//             }

//             // 空转时适当 sleep，避免 busy loop
//             if round.is_empty() {
//                 tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
//                 continue;
//             }

//             // 实际执行任务
//             for task in round {
//                 let task_id = task.id.clone();
//                 if running_tasks.insert(task_id.clone()) {
//                     let semaphore = Arc::clone(&semaphore);
//                     let running_tasks = Arc::clone(&running_tasks);

//                     let permit = semaphore.acquire_owned().await.unwrap();
//                     tokio::spawn(async move {
//                         TaskManager::process_single_task(task, running_tasks).await;
//                         drop(permit);
//                     });

//                     // 可选小延迟避免瞬时并发冲击
//                     tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
//                 }
//             }
//         }
//     });
//     external_tx
// }
