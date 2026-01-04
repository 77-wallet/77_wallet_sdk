// scanner.rs
// 🔴 核心设计原则（**必须严格遵守，否则将导致不可恢复的数据破坏**）
// 🔴 1. Scanner 只基于 DB 事实推进状态
// 🔴 2. DB 中不再依赖"正在做"的动作状态做决策
// 🔴 3. Worker/Executor 永远不参与状态决策
// 🔴 4. 系统可重启、可重复扫描、可自愈
// 🔴 5. Create/Init操作必须幂等，否则Scanner并发不安全
// 🔴 6. 禁止引入 wait_system_ready 作为全局门闩
// 🔴 7. 禁止用内存标记代替 DB 事实
// 🔴 8. 状态推进只能由 Scanner 基于 DB 强事实触发
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use futures::FutureExt;

use sqlx::SqlitePool;
use tracing::instrument;

use crate::{
    error::service::ServiceError,
    infrastructure::expand_address::{
        event::ExpandEvent,
        planner::ExpandPlanner,
        worker::{ExpandJob, WORKER_POOL},
    },
};
use wallet_database::{
    entities::expand_batch::ExpandBatchEntity,
    repositories::api_wallet::{
        expand_batch::ExpandBatchRepo, expand_batch_item::ExpandBatchItemRepo,
        wallet::ApiWalletRepo,
    },
};
use wallet_utils::address::AccountIndexMap;

/// 每个 Job 中最大的 indices 数量
/// 限制单个 Job 的大小，防止一次发送过多请求
/// buffer 的生命周期仅限于：
/// - 单次 scan_creating_items_by_account_existence
/// - 单个 batch
/// - 单轮 scan
const MAX_INDICES_PER_JOB: usize = 50;

/// ExpandScanner - 定时扫描并推进状态，遵循严格的节流语义
///
/// � 核心语义：
/// - ExpandScanner is NOT a real-time system.
/// - Progress is eventual and throttled by design.
/// - scan_interval 是吞吐控制参数，不是 SLA 参数
///
/// 🔒 不变量：
/// - Scanner不创建Item，Item的创建权只属于Planner
/// - Scanner只基于不可逆事实追平Batch状态，Batch状态由Planner和Item完成状态驱动
/// - Scanner处理Running和Done状态的Batch
/// - address_query_state是扩容系统的唯一时间闸门
/// - 扩容系统永远不尝试与查询系统并发协作，只接受其最终事实
///
/// 🔴 核心驱动：
/// - 每N秒执行一次扫描
/// - 扫描所有非Done/Failed的items
/// - 基于DB事实推进状态
/// - 派生batch状态
/// - recover机制：启动时立即执行一次扫描
///
/// 🔴 核心约束：
/// 1. 状态推进规则：所有状态更新使用compare-and-swap
/// 2. **节流语义**：单轮扫描设置上限，通过多轮扫描完成全量推进
/// 3. finished_count仅为缓存字段：不参与业务判断，只用于展示
/// 4. **事实驱动**：所有状态推进基于现有数据库实体，不依赖外部事件
/// 5. **幂等性要求**：Create/Init操作必须幂等，否则Scanner并发不安全
///
/// 🔴 单轮上限/节流机制：
/// - **max_items_per_scan**：每轮扫描处理的最大item数量（默认100）
/// - **分页处理**：使用LIMIT/OFFSET或cursor-based分页避免单次扫描压力
/// - **分批推进**：多轮扫描完成全量状态推进
/// - **资源保护**：防止DB/节点/RPC被瞬间高并发请求打爆
/// - **自适应调整**：可根据系统负载动态调整单轮上限
///
/// 🔴 设计意图：
/// - 防止重启时一次性扫描大量items导致系统过载
/// - 平滑系统负载，避免资源峰值
/// - 提高系统在大规模数据场景下的可靠性
/// - 支持水平扩展，可通过增加扫描频率而非单次处理量来提升吞吐量
/// - 便于监控和调试，单轮处理量可控
/// - 确保系统可恢复性，不依赖历史状态
///
/// 🔴 关键设计原则：
/// - Scanner scans ALL items that are not Done/Failed
/// - Status does NOT participate in dispatch decision
/// - Status is only a convergence marker
/// - This scanner is a fact reconciler, not a workflow engine
/// - It may re-dispatch side effects multiple times
/// - Correctness relies solely on DB facts and idempotency
pub struct ExpandScanner {
    pool: Arc<SqlitePool>,
    scan_interval: Duration,
    planner: ExpandPlanner,
    max_items_per_scan: u32, // 单轮扫描上限
    event_rx: Option<tokio::sync::mpsc::Receiver<ExpandEvent>>, // 事件接收器，支持事件触发扫描
    need_scan: Arc<AtomicBool>, // 标记是否需要扫描
    // Atomic flag used only for wake-up coalescing.
    // Does NOT guard any shared data, so Relaxed ordering is sufficient.
    notify: Arc<tokio::sync::Notify>, // 通知器，用于唤醒扫描循环
    // 添加原子变量控制并发
    scanning: AtomicBool,
}

impl ExpandScanner {
    pub fn new(
        pool: Arc<SqlitePool>,
        scan_interval: Duration,
        max_items_per_scan: u32,
        event_rx: Option<tokio::sync::mpsc::Receiver<ExpandEvent>>,
    ) -> Self {
        // 先克隆pool，避免移动后借用
        let planner = ExpandPlanner::new(pool.clone(), None);
        let need_scan = Arc::new(AtomicBool::new(false));
        let notify = Arc::new(tokio::sync::Notify::new());
        Self {
            pool,
            scan_interval,
            planner,
            max_items_per_scan,
            event_rx,
            need_scan,
            notify,
            scanning: AtomicBool::new(false), // 初始化原子变量
        }
    }

    /// 启动扫描器，开始定时执行
    pub async fn start(mut self) {
        tracing::info!(interval = ?self.scan_interval, max_items_per_scan = self.max_items_per_scan, "ExpandScanner: starting");

        // Invariant: scan() is never executed concurrently
        // 🔒 不变量：scan()方法永远不会被并发执行

        // recover机制：启动时立即执行一次完整扫描
        // 🔒 统一语义：start()调用recover()，而不是直接调用scan()
        if let Err(e) = self.recover().await {
            tracing::error!(error = %e, "ExpandScanner: initial recover failed");
        }

        // 先获取需要的字段
        let event_rx = self.event_rx.take();
        let need_scan = self.need_scan.clone();
        let notify = self.notify.clone();
        let scan_interval = self.scan_interval;

        // 分离self的所有权，用于扫描循环
        let scan_self = Arc::new(self);

        // 克隆Arc变量，用于扫描循环
        let scan_need_scan = need_scan.clone();
        let scan_notify = notify.clone();

        // 启动扫描循环
        tokio::spawn(async move {
            loop {
                // 等待通知
                scan_notify.notified().await;

                // 检查并重置标记位
                if !scan_need_scan.swap(false, Ordering::Relaxed) {
                    continue; // 若没有新的scan请求（事件或定时），跳过
                }

                // 使用原子变量检查是否已有scan在运行
                if scan_self.scanning.swap(true, Ordering::Relaxed) {
                    continue; // 已有scan在运行，跳过
                }

                // 执行扫描，添加panic兜底
                // 移除二次spawn，直接执行scan，减少tokio task噪音
                let result = std::panic::AssertUnwindSafe(scan_self.scan()).catch_unwind().await;

                match result {
                    Ok(inner_result) => {
                        if let Err(e) = inner_result {
                            tracing::error!(error = %e, "ExpandScanner: scan failed");
                        }
                    }
                    Err(panic) => {
                        tracing::error!(panic = ?panic, "ExpandScanner: scan panicked");
                    }
                }

                // 扫描完成，释放标记
                scan_self.scanning.store(false, Ordering::Relaxed);
            }
        });

        // 定时扫描
        let mut interval = tokio::time::interval(scan_interval);

        if let Some(mut event_rx) = event_rx {
            // 双驱动模式：事件 + 定时
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        tracing::debug!("ExpandScanner: triggered by interval");
                        need_scan.store(true, Ordering::Relaxed);
                        notify.notify_one();
                    },
                    Some(event) = event_rx.recv() => {
                        tracing::debug!(?event, "ExpandScanner: triggered by event");
                        need_scan.store(true, Ordering::Relaxed);
                        notify.notify_one();
                    },
                }
            }
        } else {
            // 传统单驱动模式：仅定时
            loop {
                interval.tick().await;
                tracing::debug!("ExpandScanner: triggered by interval (single mode)");
                need_scan.store(true, Ordering::Relaxed);
                notify.notify_one();
            }
        }
    }

    /// 执行一次完整扫描，严格遵循节流语义
    ///
    /// 🔴 核心流程：
    /// 0. 调用 Planner（唯一允许推进 Pending → Running 的组件，Scanner 不参与任何决策）
    /// 1. 扫描所有非 Done / Failed 的 items
    /// 2. 基于 DB 强事实判断：
    ///    - account 不存在 → 派发 Create 副作用
    ///    - account 存在但未 init → 派发 Init 副作用
    ///    - account 已 init → 推进到 Done
    /// 3. 更新batch状态和finished_count缓存
    #[instrument(skip(self))]
    pub async fn scan(&self) -> Result<(), ServiceError> {
        tracing::info!(
            max_items_per_scan = self.max_items_per_scan,
            "ExpandScanner: starting scan with throttling"
        );

        // 0. Planner：推进 Pending Batch → Running + create items
        // 🔒 核心逻辑：Planner是系统的"启动电机"，负责创建Item
        // 🔒 NOTE：Planner在概念上是独立于Scanner的组件
        // 🔒 在这里调用只是为了恢复方便，而非Scanner的核心职责
        if let Err(e) = self.planner.plan_all_batches().await {
            tracing::error!(error = %e, "ExpandScanner: planner failed");
            // Planner失败不影响后续扫描
            // Planner failure does NOT block Scanner progress.
            // Scanner always reconciles existing facts.
        }

        // 🔒 修复3：Scanner的processed_items计数问题
        // 🔒 补充修订B：Scanner的quota应该是「一次scan的全局硬上限」，所有scan_xxx()都消耗它
        // 🔒 全局processed_items计数器，用于限制单轮扫描的总items数量
        // 🔒 设计权衡：所有未完成状态共用quota
        // 🔒 优点：实现简单，全局控制资源使用
        // 🔒 缺点：可能导致饥饿问题，但基于事实驱动，不会永久阻塞
        let mut processed_items = 0;

        // 2. 扫描所有未完成的items，基于DB事实推进状态（不依赖任何中间状态）
        self.scan_unfinished_items_by_db_fact(&mut processed_items).await?;

        // 3. 执行batch状态派生（更新finished_count缓存）
        self.scan_batches().await?;

        // 4. 处理所有Done状态的批次，派发通知任务
        // IMPORTANT:
        // handle_done_batches MUST be called after scan_batches()
        // because Done → Notified is derived strictly from DB facts
        self.handle_done_batches().await?;

        tracing::info!(
            processed_items = processed_items,
            "ExpandScanner: scan completed with throttling"
        );
        Ok(())
    }

    /// 扫描所有非Done/Failed状态的items，基于DB事实推进状态
    ///
    /// 🔴 核心设计原则：
    /// - Scanner scans ALL items that are not Done/Failed
    /// - Status does NOT participate in dispatch decision
    /// - Status is only a convergence marker
    /// - This scanner is a fact reconciler, not a workflow engine
    /// - It may re-dispatch side effects multiple times
    /// - Correctness relies solely on DB facts and idempotency
    ///
    /// 🔴 Create/Init操作必须幂等，否则Scanner并发不安全
    /// 🔴 原因：Scanner可能并发执行或多次执行，同一个item可能被重复发送create/init任务
    /// 🔴 后果：若Create/Init操作非幂等，将导致不可恢复的数据破坏和状态不一致
    /// 🔴 这是当前设计必须依赖的前提，不是未来优化项
    ///
    /// 🔴 参数说明：
    /// - processed_items: 全局计数器，用于限制单轮扫描的总items数量
    /// - 公平性权衡：
    ///   ✅ 优点：实现简单，全局控制资源使用，避免单batch无限占用
    ///   ❌ 缺点：可能导致饥饿问题（某些batch长期被挤出扫描窗口）
    ///   ⚠️  设计决策：这是明确的trade-off，基于事实驱动的设计不会导致永久阻塞
    ///   ⚠️  未来优化：可根据实际情况调整为更公平的quota分配策略
    ///
    /// NOTE:
    /// This method does NOT guarantee fairness across batches.
    /// Ordering is best-effort and bounded by global quota.
    /// 防止未来有人问："为什么这个batch总是慢？"
    async fn scan_unfinished_items_by_db_fact(
        &self,
        processed_items: &mut usize,
    ) -> Result<(), ServiceError> {
        tracing::info!("ExpandScanner: scanning unfinished items by DB fact");

        // 获取需要进行item reconciliation的批次（事实驱动）
        // 只处理 status 为 Running 但 local_complete_at 已设置的批次
        let batches = ExpandBatchRepo::get_batches_for_item_reconcile(self.pool.clone()).await?;

        for batch in batches {
            // 🔒 设计：全局 quota + 顺序 batch 扫描
            // 🔒 非严格公平，仅避免单 batch 无限占用
            // 🔒 计算当前batch可用的剩余quota
            // NOTE: processed_items 是全局上限，batch_processed 是当前 batch 的软上限
            // ⚠️ 已知 trade-off：此设计可能导致某些 batch 长期被挤出扫描窗口（饥饿问题）
            // ⚠️ 接受原因：为了正确性和可恢复性，优先保证系统稳定
            let remaining_quota =
                self.max_items_per_scan.saturating_sub(*processed_items as u32) as usize;
            if remaining_quota == 0 {
                tracing::info!(
                    processed_items = *processed_items,
                    max_items_per_scan = self.max_items_per_scan,
                    "ExpandScanner: reached max items per scan, stopping"
                );
                break;
            }

            // Scanner scans ALL items that are not Done/Failed
            // Status does NOT participate in dispatch decision
            // Status is only a convergence marker
            let unfinished_items =
                ExpandBatchItemRepo::list_unfinished_items(self.pool.clone(), &batch.batch_id)
                    .await?;

            // 🔒 使用剩余quota限制每个batch处理的items数量
            let mut batch_processed = 0;

            // 临时buffer，用于批量发送Job
            // buffer的生命周期仅限于：
            // - 单次scan_unfinished_items_by_db_fact
            // - 单个batch
            // - 单轮scan
            let mut init_indices = Vec::new();
            let mut create_indices = Vec::new();
            let mut done_indices = Vec::new();

            for item in unfinished_items {
                // 检查是否达到单轮上限
                if *processed_items >= self.max_items_per_scan as usize {
                    tracing::info!(
                        processed_items = *processed_items,
                        max_items_per_scan = self.max_items_per_scan,
                        "ExpandScanner: reached max items per scan, stopping"
                    );
                    break;
                }

                // 检查账户是否已创建（直接查询api_account表，使用点查，避免O(N)查询）
                let account_exists = self
                    .check_account_exists(&item.uid, &item.chain_code, item.input_index)
                    .await?;

                // 检查账户是否已初始化
                let address_inited = if account_exists {
                    self.check_address_inited(&item.uid, &item.chain_code, item.input_index).await?
                } else {
                    false
                };

                // 基于DB事实推进状态
                if !account_exists {
                    // 🔴 事实硬闸：只有当账户确实不存在时，才发送创建任务
                    // 避免在事实已存在时重复派发副作用
                    // Scanner 只看事实，不看状态
                    tracing::debug!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: account not found, sending create job");

                    // 发送创建任务
                    tracing::info!(batch_id = %item.batch_id, index = item.input_index, "SCANNER: dispatching expand job - Create");
                    create_indices.push(item.input_index);
                    *processed_items += 1;
                    batch_processed += 1;
                } else if !address_inited {
                    // 🔴 事实硬闸：只有当账户确实未初始化时，才发送初始化任务
                    // 避免在事实已存在时重复派发副作用
                    // Scanner 只看事实，不看状态
                    tracing::debug!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: account exists but not init, sending init job");

                    // 发送初始化任务
                    tracing::info!(batch_id = %item.batch_id, index = item.input_index, "SCANNER: dispatching expand job - Init");
                    init_indices.push(item.input_index);
                    *processed_items += 1;
                    batch_processed += 1;
                } else {
                    // 🔴 事实硬闸：账户已初始化 → 推进到Done
                    // 基于强事实（is_init=1），无论当前状态是什么，都推进到Done
                    tracing::debug!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: account exists and init, marking as Done");

                    // 推进到Done
                    let updated = ExpandBatchItemRepo::dispatched_to_done_if_fact_match(
                        self.pool.clone(),
                        &item.batch_id,
                        &[item.input_index],
                    )
                    .await?;

                    if updated > 0 {
                        done_indices.push(item.input_index);
                        *processed_items += 1;
                        batch_processed += 1;
                        tracing::info!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: marked as Done");
                    } else {
                        tracing::info!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: failed to mark as Done, item status may have changed");
                    }
                }

                // 🔒 确保每个batch只使用分配的quota
                if batch_processed >= remaining_quota {
                    tracing::info!(batch_id = %batch.batch_id, batch_processed = batch_processed, remaining_quota = remaining_quota, "ExpandScanner: batch reached its quota, moving to next batch");
                    break;
                }
            }

            tracing::info!("ExpandScanner: init_indices: {:?}", init_indices);
            // 批量发送初始化任务
            if !init_indices.is_empty() {
                self.send_init_jobs_batch(&batch, &init_indices).await?;
            }
            tracing::info!("ExpandScanner: create_indices: {:?}", create_indices);

            // 批量发送创建任务
            if !create_indices.is_empty() {
                self.send_create_jobs_batch(&batch, &create_indices).await?;
            }
        }

        tracing::info!(
            processed_items = *processed_items,
            "ExpandScanner: completed scanning unfinished items by DB fact"
        );
        Ok(())
    }

    /// 检查账户是否已创建（使用点查，避免O(N)查询）
    ///
    /// 🔴 热路径IO优化点：
    /// - 该方法在Scanner的热路径上，每次调用都会执行两次数据库查询
    /// - 优化建议：
    ///   1. 考虑添加缓存层，缓存uid→wallet和wallet+chain+index→account的映射关系
    ///   2. 批量处理：将多个check_account_exists调用合并为一次批量查询
    ///   3. 异步并行：使用tokio::spawn或join_all并行执行多个check_account_exists请求
    ///   4. 数据库索引优化：确保相关查询都有合适的索引支持
    /// - 当前性能特点：
    ///   ✅ 使用点查避免O(N)查询
    ///   ✅ 单个请求响应时间可接受
    ///   ❌ 高并发场景下可能成为瓶颈
    async fn check_account_exists(
        &self,
        uid: &str,
        chain: &str,
        index: i32,
    ) -> Result<bool, ServiceError> {
        tracing::info!(uid=%uid, chain=%chain, input_index=%index, "ExpandScanner: checking account existence");
        let pool = self.pool.clone();

        // 获取api_wallet
        let wallet = ApiWalletRepo::find_by_uid(pool.clone(), uid).await?;

        if let Some(wallet) = wallet {
            // 🔒 修复4：使用正确的AccountIndexMap将input_index转换为account_id
            // 🔒 不再假设input_index直接对应account_id，避免数据一致性问题
            let index_map = AccountIndexMap::from_input_index(index)?;
            let expected_account_id = index_map.account_id;
            tracing::info!(uid=%uid, chain=%chain, input_index=%index, expected_account_id=%expected_account_id, wallet_address=%wallet.address, "ExpandScanner: converted input_index to account_id");

            // 查询特定账户和chain_code的api_account记录是否存在
            // 使用点查，避免O(N)查询
            let accounts = wallet_database::repositories::api_wallet::account::ApiAccountRepo::find_all_by_wallet_address_index(
                pool.clone(),
                &wallet.address,
                chain,
                expected_account_id
            ).await?;
            tracing::info!(uid=%uid, chain=%chain, input_index=%index, expected_account_id=%expected_account_id, accounts_found=%accounts.len(), "ExpandScanner: account existence check result");

            Ok(!accounts.is_empty())
        } else {
            tracing::info!(uid=%uid, chain=%chain, input_index=%index, "ExpandScanner: wallet not found");
            Ok(false)
        }
    }

    /// 检查地址是否已初始化（使用点查，避免O(N)查询）
    ///
    /// 🔴 热路径IO优化点：
    /// - 该方法在Scanner的热路径上，每次调用都会执行两次数据库查询
    /// - 优化建议：
    ///   1. 考虑添加缓存层，缓存uid→wallet和wallet+chain+index→account的映射关系
    ///   2. 批量处理：将多个check_address_inited调用合并为一次批量查询
    ///   3. 异步并行：使用tokio::spawn或join_all并行执行多个check_address_inited请求
    ///   4. 数据库索引优化：确保相关查询都有合适的索引支持
    ///   5. 与check_account_exists合并：两个方法执行类似的查询，可以考虑合并为一个方法减少IO
    /// - 当前性能特点：
    ///   ✅ 使用点查避免O(N)查询
    ///   ✅ 单个请求响应时间可接受
    ///   ❌ 高并发场景下可能成为瓶颈
    ///   ❌ 与check_account_exists存在重复查询，可进一步优化
    async fn check_address_inited(
        &self,
        uid: &str,
        chain: &str,
        index: i32,
    ) -> Result<bool, ServiceError> {
        tracing::info!(uid=%uid, chain=%chain, input_index=%index, "ExpandScanner: checking address initialization status");
        let pool = self.pool.clone();

        // 获取api_wallet
        let wallet = ApiWalletRepo::find_by_uid(pool.clone(), uid).await?;

        if let Some(wallet) = wallet {
            // 🔒 修复4：使用正确的AccountIndexMap将input_index转换为account_id
            // 🔒 不再假设input_index直接对应account_id，避免数据一致性问题
            let index_map = AccountIndexMap::from_input_index(index)?;
            let expected_account_id = index_map.account_id;
            tracing::info!(uid=%uid, chain=%chain, input_index=%index, expected_account_id=%expected_account_id, wallet_address=%wallet.address, "ExpandScanner: converted input_index to account_id");

            // 查询特定账户和chain_code的api_account记录，检查是否已初始化
            // 使用点查，避免O(N)查询
            let accounts = wallet_database::repositories::api_wallet::account::ApiAccountRepo::find_all_by_wallet_address_index(
                pool.clone(),
                &wallet.address,
                chain,
                expected_account_id
            ).await?;
            tracing::info!(uid=%uid, chain=%chain, input_index=%index, expected_account_id=%expected_account_id, accounts_found=%accounts.len(), "ExpandScanner: account initialization check - accounts found");

            // 检查每个账户的is_init状态
            for account in &accounts {
                tracing::info!(uid=%uid, chain=%chain, address=%account.address, is_init=%account.is_init, "ExpandScanner: account initialization status for address");
            }

            // 检查是否存在已初始化的记录
            let is_inited =
                !accounts.is_empty() && accounts.iter().any(|account| account.is_init == 1);
            tracing::info!(uid=%uid, chain=%chain, input_index=%index, expected_account_id=%expected_account_id, is_inited=%is_inited, "ExpandScanner: address initialization check result");
            Ok(is_inited)
        } else {
            tracing::info!(uid=%uid, chain=%chain, input_index=%index, "ExpandScanner: wallet not found");
            Ok(false)
        }
    }

    /// 批量发送创建账户任务
    async fn send_create_jobs_batch(
        &self,
        batch: &wallet_database::entities::expand_batch::ExpandBatchEntity,
        indices: &[i32],
    ) -> Result<(), ServiceError> {
        // IMPORTANT:
        // Scanner does NOT advance item status when dispatching Create/Init.
        // State convergence relies solely on DB facts observed in later scans.
        // 防止未来有人误以为："发了job就等于推进了状态"
        tracing::info!(batch_id = %batch.batch_id, indices_count = indices.len(), "ExpandScanner: sending batch create jobs");

        // 分批发送，每次不超过 MAX_INDICES_PER_JOB
        for chunk in indices.chunks(MAX_INDICES_PER_JOB) {
            let job = ExpandJob::new_create(
                batch.uid.clone(),
                batch.chain_code.clone(),
                batch.batch_id.clone(),
                chunk.to_vec(),
            );

            // 使用try_send替代await send，避免阻塞
            match WORKER_POOL.tx.try_send(job) {
                Ok(_) => {
                    // 任务发送成功，正常处理
                    tracing::debug!(batch_id = %batch.batch_id, chunk_size = chunk.len(), "ExpandScanner: sent create job chunk successfully");
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    // 任务队列已满，记录警告日志并继续处理
                    // 状态已经推进，任务会在下轮扫描中重试
                    tracing::warn!(batch_id = %batch.batch_id, chunk_size = chunk.len(), "ExpandScanner: worker pool full, skipped create job chunk, will retry in next scan");
                    // 继续处理下一个chunk，不break
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    // 任务队列已关闭，记录错误日志并继续处理
                    // 状态已经推进，不允许回滚
                    tracing::error!(batch_id = %batch.batch_id, chunk_size = chunk.len(), "ExpandScanner: worker pool closed, skipped create job chunk, state already advanced");
                    // 继续处理下一个chunk，不break
                }
            }
        }
        Ok(())
    }

    /// 批量发送初始化账户任务
    async fn send_init_jobs_batch(
        &self,
        batch: &wallet_database::entities::expand_batch::ExpandBatchEntity,
        indices: &[i32],
    ) -> Result<(), ServiceError> {
        // IMPORTANT:
        // Scanner does NOT advance item status when dispatching Create/Init.
        // State convergence relies solely on DB facts observed in later scans.
        // 防止未来有人误以为："发了job就等于推进了状态"
        tracing::info!(batch_id = %batch.batch_id, indices_count = indices.len(), "ExpandScanner: sending batch init jobs");

        // 分批发送，每次不超过 MAX_INDICES_PER_JOB
        for chunk in indices.chunks(MAX_INDICES_PER_JOB) {
            let job = ExpandJob::new_init(
                batch.uid.clone(),
                batch.chain_code.clone(),
                batch.batch_id.clone(),
                chunk.to_vec(),
            );

            // 使用try_send替代await send，避免阻塞
            match WORKER_POOL.tx.try_send(job) {
                Ok(_) => {
                    // 任务发送成功，正常处理
                    tracing::debug!(batch_id = %batch.batch_id, chunk_size = chunk.len(), "ExpandScanner: sent init job chunk successfully");
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    // 任务队列已满，记录警告日志并继续处理
                    // 状态已经推进，任务会在下轮扫描中重试
                    tracing::warn!(batch_id = %batch.batch_id, chunk_size = chunk.len(), "ExpandScanner: worker pool full, skipped init job chunk, will retry in next scan");
                    // 继续处理下一个chunk，不break
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    // 任务队列已关闭，记录错误日志并继续处理
                    // 状态已经推进，不允许回滚
                    tracing::error!(batch_id = %batch.batch_id, chunk_size = chunk.len(), "ExpandScanner: worker pool closed, skipped init job chunk, state already advanced");
                    // 继续处理下一个chunk，不break
                }
            }
        }
        Ok(())
    }

    /// 扫描并派生batch状态
    /// 扫描所有批次，进行状态追平和缓存更新
    ///
    /// 🔴 核心职责：
    /// - 仅负责批次状态的追平（Running → Done）
    /// - 仅负责更新finished_count缓存
    /// - 不负责任何notify任务的派发
    /// - 所有状态更新都基于不可逆事实
    ///
    /// Does NOT scan Done batches.
    /// Done → Notified is handled separately in handle_done_batches()
    #[instrument(skip(self))]
    async fn scan_batches(&self) -> Result<(), ServiceError> {
        tracing::info!("ExpandScanner: scanning batches");

        // 1. 获取所有状态为Running的批次，用于状态追平
        let running_batches = ExpandBatchRepo::get_by_status(
            self.pool.clone(),
            wallet_database::entities::expand_batch::ExpandBatchStatus::Running,
        )
        .await?;

        // 2. 更新每个批次的finished_count缓存
        for batch in running_batches {
            // 2.1 重新计算finished_count（仅作为缓存）
            let count =
                ExpandBatchItemRepo::count_done_items(self.pool.clone(), &batch.batch_id).await?;

            // 2.2 更新finished_count
            // finished_count is a derived cache.
            // Rewriting it multiple times is expected and correct.
            // 防止未来有人想加CAS/乐观锁
            ExpandBatchRepo::update_finished_count_cache_only(
                self.pool.clone(),
                &batch.batch_id,
                count,
            )
            .await?;

            // 2.3 检查本地扩容是否已完成（基于local_complete_at事实）
            let is_local_completed =
                ExpandBatchRepo::is_local_completed(self.pool.clone(), &batch.batch_id).await?;

            // 3. 如果本地扩容已完成，推进batch状态到Done（事实驱动）
            if is_local_completed {
                // 使用mark_done_if_local_completed方法，该方法已经包含了CAS保护
                // 条件：local_complete_at IS NOT NULL AND status = Running
                let updated = ExpandBatchRepo::mark_done_if_local_completed(
                    self.pool.clone(),
                    &batch.batch_id,
                )
                .await?;
                if updated > 0 {
                    tracing::info!(batch_id = %batch.batch_id, affected_rows = updated, "ExpandScanner: batch marked as Done based on local_complete_at fact");
                } else {
                    tracing::debug!(batch_id = %batch.batch_id, "ExpandScanner: batch already marked as Done or local_complete_at not set, skipping");
                }
            }
        }

        Ok(())
    }

    /// 检查并发送通知任务
    ///
    /// 🔴 前置条件（必须严格遵守）：
    /// - 传入的批次必须满足notify派发条件
    /// - Batch.status ∈ {Done}
    /// - 且 "通知事实尚未形成"（expand_complete_at IS NULL）
    /// - 该条件已经在调用方handle_single_done_batch中检查过
    ///
    /// 🔴 严格禁止：
    /// This method MUST NOT perform any fact checking.
    /// Fact reconciliation must happen before calling this method.
    /// 禁止在此方法中添加任何事实检查逻辑
    ///
    /// 事实驱动的通知分发：
    /// - 优先基于expand_complete_at事实
    /// - 确保幂等性，避免重复通知
    /// - 通知的幂等性只依赖事实字段，不依赖状态
    ///
    /// 注意：expand_complete_at 表示【已成功上报完成】，不是本地扩容完成
    async fn dispatch_notify_job_if_needed(
        &self,
        batch: &ExpandBatchEntity,
    ) -> Result<(), ServiceError> {
        // 3. 发送通知任务
        let job = ExpandJob::new_notify(
            batch.uid.clone(),
            batch.chain_code.clone(),
            batch.batch_id.clone(),
        );

        // 记录notify job分发
        tracing::info!(batch_id = %batch.batch_id, "SCANNER: dispatching expand job - Notify");

        // 使用try_send替代await send，避免阻塞
        match WORKER_POOL.tx.try_send(job) {
            Ok(_) => {
                tracing::debug!(batch_id = %batch.batch_id, "ExpandScanner: sent notify job successfully");
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(batch_id = %batch.batch_id, "ExpandScanner: worker pool full, skipped notify job, will retry in next scan");
                // 继续执行，不返回错误，下一轮扫描会重试
            }
            Err(e) => {
                tracing::warn!(batch_id = %batch.batch_id, error = %e, "ExpandScanner: failed to send notify job");
                // 继续执行，不返回错误，下一轮扫描会重试
            }
        }

        Ok(())
    }

    /// 处理所有Done状态的批次，派发通知任务
    ///
    /// 🔴 核心职责：
    /// - 扫描所有status = Done的批次
    /// - 派发扩容完成通知任务
    /// - 不负责推进到Notified状态
    /// - Notified状态只能由通知执行者在成功后推进
    ///
    /// NOTE:
    /// This is the ONLY place where notify jobs are dispatched.
    /// Scanner must never dispatch notify jobs elsewhere.
    /// 这是防未来误改的「保险丝」
    #[instrument(skip(self))]
    async fn handle_done_batches(&self) -> Result<(), ServiceError> {
        tracing::info!("ExpandScanner: handling done batches");

        // 获取所有Done状态的批次
        let done_batches = ExpandBatchRepo::get_all_done(self.pool.clone()).await?;

        for batch in done_batches {
            // 串行处理每个batch，避免并发重复执行expand_complete
            if let Err(e) = self.handle_single_done_batch(&batch).await {
                tracing::error!(batch_id = %batch.batch_id, error = %e, "ExpandScanner: failed to handle done batch");
            }
        }

        Ok(())
    }

    /// 处理单个Done状态的批次
    async fn handle_single_done_batch(
        &self,
        batch: &ExpandBatchEntity,
    ) -> Result<(), ServiceError> {
        tracing::info!(batch_id = %batch.batch_id, "ExpandScanner: handling single done batch");

        // Precondition:
        // - batch.status = Done
        // - expand_complete_at IS NULL
        // Scanner只负责派发通知任务，不负责推进到Notified状态
        // Notified状态只能由通知执行者推进

        // 检查是否已经通知完成（事实已形成）
        let is_expand_completed =
            ExpandBatchRepo::is_batch_notified_fact(self.pool.clone(), &batch.batch_id).await?;
        if is_expand_completed {
            tracing::debug!(batch_id = %batch.batch_id, "ExpandScanner: batch already notified, skipping notification dispatch");
            ExpandBatchRepo::done_to_notified_if_match(self.pool.clone(), &batch.batch_id).await?;
            return Ok(());
        }

        // 派发通知任务，由通知执行者在成功后推进到Notified状态
        self.dispatch_notify_job_if_needed(batch).await?;

        Ok(())
    }

    /// Recover机制：启动时立即执行一次扫描
    ///
    /// 🔴 核心语义：
    /// recover() ≠ 修复
    /// recover() = 对所有可能推进的状态做一次完整扫描
    /// recover() is semantically equivalent to a normal scan()
    /// The only difference is invocation timing.
    /// 防止未来有人往recover里加"特殊逻辑"
    ///
    /// 包含所有扫描步骤：
    /// 1. 调用Planner，处理Pending Batch（可能推进 Pending → Running 并创建 items）
    /// 2. 扫描所有未完成 items，基于 DB 事实对齐状态
    /// 3. 更新batch状态和finished_count缓存
    pub async fn recover(&mut self) -> Result<(), ServiceError> {
        tracing::info!("ExpandScanner: starting recover - performing full scan");

        // scanning is a coarse-grained mutex to guarantee:
        // - scan() is never executed concurrently
        // - recover() and periodic scan share the same exclusion domain
        // 使用原子变量检查是否已有scan在运行
        if self.scanning.swap(true, Ordering::Relaxed) {
            tracing::info!("ExpandScanner: recovery skipped, scan already running");
            return Ok(()); // 已有scan在运行，跳过
        }

        // 添加panic兜底，确保scanning原子位不会永久卡死
        let result = std::panic::AssertUnwindSafe(self.scan()).catch_unwind().await;

        let scan_result = match result {
            Ok(inner_result) => inner_result,
            Err(panic) => {
                tracing::error!(panic = ?panic, "ExpandScanner: recover panicked");
                // 扫描完成，释放标记
                self.scanning.store(false, Ordering::Relaxed);
                return Err(ServiceError::System(crate::error::system::SystemError::Internal(
                    "recover panicked".into(),
                )));
            }
        };

        // 扫描完成，释放标记
        self.scanning.store(false, Ordering::Relaxed);

        scan_result?;

        tracing::info!("ExpandScanner: recover completed");
        Ok(())
    }
}
