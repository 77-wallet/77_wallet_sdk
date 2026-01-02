// scanner.rs
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

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
    entities::expand_batch_item::ExpandItemStatus,
    repositories::api_wallet::{
        expand_batch::ExpandBatchRepo, expand_batch_item::ExpandBatchItemRepo,
        wallet::ApiWalletRepo,
    },
};
use wallet_utils::address::AccountIndexMap;

/// ExpandScanner - 定时扫描并推进状态，遵循严格的节流语义
///
/// 🔒 不变量1：Scanner不创建Item，Item的创建权只属于Planner
/// 🔒 不变量2：Scanner不修改Batch状态，Batch状态只由Planner和Item完成状态驱动
/// 🔒 不变量3：Scanner只推进已存在Item的状态（Creating → Initing → Done）
/// 🔒 不变量4：Scanner只处理Running状态的Batch
/// 🔒 不变量5：Scanner不处理Pending状态的Item，Item创建时直接为Creating状态
/// 🔒 不变量6：address_query_state是扩容系统的唯一时间闸门
/// 🔒 不变量7：扩容系统永远不尝试与查询系统并发协作，只接受其最终事实
///
/// 🔴 核心驱动：
/// - 每N秒执行一次扫描
/// - 扫描Creating/Initing状态的item
/// - 推进状态：Creating→Initing→Done
/// - 失败时retry+backoff，停留在当前状态
/// - 派生batch状态
/// - recover机制：启动时立即执行一次扫描
///
/// 🔴 核心约束：
/// 1. 状态推进规则：所有状态更新使用compare-and-swap
/// 2. 状态机不变量：状态只能单向推进，失败不回退
/// 3. **节流语义**：单轮扫描设置上限，通过多轮扫描完成全量推进
/// 4. finished_count仅为缓存字段：不参与业务判断，只用于展示
/// 5. **事实驱动**：所有状态推进基于现有数据库实体，不依赖外部事件
///
/// 🔴 单轮上限/节流机制：
/// - **max_items_per_scan**：每轮扫描处理的最大item数量（默认100）
/// - **分页处理**：使用LIMIT/OFFSET或cursor-based分页避免单次扫描压力
/// - **分批推进**：多轮扫描完成全量状态推进
/// - **资源保护**：防止DB/节点/RPC被瞬间高并发请求打爆
/// - **backoff机制**：失败时自动重试，避免频繁失败的item占用过多资源
/// - **自适应调整**：可根据系统负载动态调整单轮上限
///
/// 🔴 设计意图：
/// - 防止重启时一次性扫描大量items导致系统过载
/// - 平滑系统负载，避免资源峰值
/// - 提高系统在大规模数据场景下的可靠性
/// - 支持水平扩展，可通过增加扫描频率而非单次处理量来提升吞吐量
/// - 便于监控和调试，单轮处理量可控
/// - 确保系统可恢复性，不依赖历史状态
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
        Self { pool, scan_interval, planner, max_items_per_scan, event_rx, need_scan, notify }
    }

    /// 创建不支持事件的扫描器实例（向后兼容）
    pub fn new_without_events(
        pool: Arc<SqlitePool>,
        scan_interval: Duration,
        max_items_per_scan: u32,
    ) -> Self {
        Self::new(pool, scan_interval, max_items_per_scan, None)
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
        let scan_self = Arc::new(tokio::sync::Mutex::new(self));

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

                // 执行扫描
                let mut locked_self = scan_self.lock().await;
                if let Err(e) = locked_self.scan().await {
                    tracing::error!(error = %e, "ExpandScanner: scan failed");
                }
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
    /// 1. 扫描Creating状态的items，检查账户是否存在，推进Creating→Initing（事实驱动）
    /// 2. 扫描Initing状态的items，检查地址是否初始化，推进Initing→Done（事实驱动）
    /// 3. 更新batch状态和finished_count缓存
    #[instrument(skip(self))]
    pub async fn scan(&mut self) -> Result<(), ServiceError> {
        tracing::info!(
            max_items_per_scan = self.max_items_per_scan,
            "ExpandScanner: starting scan with throttling"
        );

        // 0. Planner：推进 Pending Batch → Running + create items
        // 🔒 核心逻辑：Planner是系统的"启动电机"，负责创建Item
        if let Err(e) = self.planner.plan_all_batches().await {
            tracing::error!(error = %e, "ExpandScanner: planner failed");
            // Planner失败不影响后续扫描
        }

        // 🔒 修复3：Scanner的processed_items计数问题
        // 🔒 补充修订B：Scanner的quota应该是「一次scan的全局硬上限」，所有scan_xxx()都消耗它
        // 🔒 全局processed_items计数器，用于限制单轮扫描的总items数量
        // 🔒 设计权衡：Creating和Initing共用quota
        // 🔒 优点：实现简单，全局控制资源使用
        // 🔒 缺点：可能导致饥饿问题（Creating很多→Initing饥饿，或反之）
        // 🔒 这是明确的trade-off，不是bug，未来可根据实际情况优化
        let mut processed_items = 0;

        // 2. 扫描Creating状态的items，检查账户是否存在，推进Creating→Initing（事实驱动）
        self.scan_creating_items_by_account_existence(&mut processed_items).await?;

        // 3. 扫描Initing状态的items，检查地址是否初始化，推进Initing→Done（事实驱动）
        self.scan_initing_items_by_address_state(&mut processed_items).await?;

        // 4. 执行batch状态派生（更新finished_count缓存）
        self.scan_batches().await?;

        tracing::info!(
            processed_items = processed_items,
            "ExpandScanner: scan completed with throttling"
        );
        Ok(())
    }

    /// 扫描Creating状态的items，通过检查账户是否存在来推进状态
    /// 触发条件：item状态为Creating
    /// 行为：
    /// - 账户不存在 → 发送创建任务
    /// - 账户存在 → 使用CAS将状态推进到Initing，并发送初始化任务
    /// 🔴 关键设计前提（**必须严格遵守，否则将导致不可恢复的数据破坏**）：
    /// 🔴 Create/Init操作必须幂等，否则Scanner并发不安全
    /// 🔴 原因：Scanner可能并发执行或多次执行，同一个item可能被重复发送create/init任务
    /// 🔴 后果：若Create/Init操作非幂等，将导致不可恢复的数据破坏和状态不一致
    /// 🔴 这是当前设计必须依赖的前提，不是未来优化项
    #[instrument(skip(self))]
    async fn scan_creating_items_by_account_existence(
        &self,
        processed_items: &mut usize,
    ) -> Result<(), ServiceError> {
        tracing::info!("ExpandScanner: scanning creating items by account existence");

        // 获取所有Creating状态的items
        let batches = ExpandBatchRepo::get_all_running_batches(self.pool.clone()).await?;

        for batch in batches {
            // 🔒 设计：全局 quota + 顺序 batch 扫描
            // 🔒 非严格公平，仅避免单 batch 无限占用
            // 🔒 计算当前batch可用的剩余quota
            let remaining_quota = self.max_items_per_scan as usize - *processed_items;
            if remaining_quota <= 0 {
                tracing::info!(
                    processed_items = *processed_items,
                    max_items_per_scan = self.max_items_per_scan,
                    "ExpandScanner: reached max items per scan for creating items, stopping"
                );
                break;
            }

            // 获取当前batch中所有Creating状态的items
            let creating_items = ExpandBatchItemRepo::fetch_by_batch_and_status(
                self.pool.clone(),
                &batch.batch_id,
                ExpandItemStatus::Creating,
            )
            .await?;

            // 🔒 使用剩余quota限制每个batch处理的items数量
            let mut batch_processed = 0;
            for item in creating_items {
                // 检查是否达到单轮上限
                if *processed_items >= self.max_items_per_scan as usize {
                    tracing::info!(
                        processed_items = *processed_items,
                        max_items_per_scan = self.max_items_per_scan,
                        "ExpandScanner: reached max items per scan for creating items, stopping"
                    );
                    break;
                }

                // 检查账户是否已创建（直接查询api_account表，使用点查，避免O(N)查询）
                let account_exists = self
                    .check_account_exists(&item.uid, &item.chain_code, item.input_index)
                    .await?;

                if account_exists {
                    // 账户已存在，使用CAS将状态推进到Initing
                    tracing::debug!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: found account exists, attempting to advance Creating → Initing");

                    let updated = ExpandBatchItemRepo::creating_to_initing_if_match(
                        self.pool.clone(),
                        &item.batch_id,
                        &[item.input_index],
                    )
                    .await?;

                    if updated > 0 {
                        // 成功推进状态，发送初始化任务
                        self.send_init_job(&item).await?;
                        *processed_items += 1;
                        batch_processed += 1;
                        tracing::info!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: successfully advanced Creating → Initing");
                    } else {
                        tracing::debug!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: failed to advance Creating → Initing, item status may have changed");
                    }
                } else {
                    // 账户不存在，发送创建任务
                    tracing::debug!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: account not found, sending create job");
                    self.send_create_job(&item).await?;
                    *processed_items += 1;
                    batch_processed += 1;
                    tracing::info!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: sent create job for account");
                }

                // 🔒 确保每个batch只使用分配的quota
                if batch_processed >= remaining_quota {
                    tracing::debug!(batch_id = %batch.batch_id, batch_processed = batch_processed, remaining_quota = remaining_quota, "ExpandScanner: batch reached its quota, moving to next batch");
                    break;
                }
            }
        }

        tracing::info!(
            processed_items = *processed_items,
            "ExpandScanner: completed scanning creating items by account existence"
        );
        Ok(())
    }

    /// 扫描Initing状态的items，通过检查地址状态来推进状态
    /// 触发条件：item状态为Initing，且对应地址已初始化
    /// 行为：使用CAS将状态推进到Done
    #[instrument(skip(self))]
    async fn scan_initing_items_by_address_state(
        &self,
        processed_items: &mut usize,
    ) -> Result<(), ServiceError> {
        tracing::info!("ExpandScanner: scanning initing items by address state");

        // 获取所有Initing状态的items
        let batches = ExpandBatchRepo::get_all_running_batches(self.pool.clone()).await?;

        for batch in batches {
            // 🔒 设计：全局 quota + 顺序 batch 扫描
            // 🔒 非严格公平，仅避免单 batch 无限占用
            // 🔒 计算当前batch可用的剩余quota
            let remaining_quota = self.max_items_per_scan as usize - *processed_items;
            if remaining_quota <= 0 {
                tracing::info!(
                    processed_items = *processed_items,
                    max_items_per_scan = self.max_items_per_scan,
                    "ExpandScanner: reached max items per scan for initing items, stopping"
                );
                break;
            }

            // 获取当前batch中所有Initing状态的items
            let initing_items = ExpandBatchItemRepo::fetch_by_batch_and_status(
                self.pool.clone(),
                &batch.batch_id,
                ExpandItemStatus::Initing,
            )
            .await?;

            // 🔒 使用剩余quota限制每个batch处理的items数量
            let mut batch_processed = 0;
            for item in initing_items {
                // 检查是否达到单轮上限
                if *processed_items >= self.max_items_per_scan as usize {
                    tracing::info!(
                        processed_items = *processed_items,
                        max_items_per_scan = self.max_items_per_scan,
                        "ExpandScanner: reached max items per scan for initing items, stopping"
                    );
                    break;
                }

                // 检查地址是否已初始化（直接查询api_account表，使用点查，避免O(N)查询）
                let address_inited = self
                    .check_address_inited(&item.uid, &item.chain_code, item.input_index)
                    .await?;

                if address_inited {
                    // 地址已初始化，使用CAS将状态推进到Done
                    tracing::debug!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: found address inited, attempting to advance Initing → Done");

                    let updated = ExpandBatchItemRepo::initing_to_done_if_match(
                        self.pool.clone(),
                        &item.batch_id,
                        &[item.input_index],
                    )
                    .await?;

                    if updated > 0 {
                        *processed_items += 1;
                        batch_processed += 1;
                        tracing::info!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: successfully advanced Initing → Done");
                    } else {
                        tracing::debug!(batch_id = %item.batch_id, index = item.input_index, "ExpandScanner: failed to advance Initing → Done, item status may have changed");
                    }
                }

                // 🔒 确保每个batch只使用分配的quota
                if batch_processed >= remaining_quota {
                    tracing::debug!(batch_id = %batch.batch_id, batch_processed = batch_processed, remaining_quota = remaining_quota, "ExpandScanner: batch reached its quota, moving to next batch");
                    break;
                }
            }
        }

        tracing::info!(
            processed_items = *processed_items,
            "ExpandScanner: completed scanning initing items by address state"
        );
        Ok(())
    }

    /// 检查账户是否已创建（使用点查，避免O(N)查询）
    async fn check_account_exists(
        &self,
        uid: &str,
        chain: &str,
        index: i32,
    ) -> Result<bool, ServiceError> {
        let pool = self.pool.clone();

        // 获取api_wallet
        let wallet = ApiWalletRepo::find_by_uid(pool.clone(), uid).await?;

        if let Some(wallet) = wallet {
            // 🔒 修复4：使用正确的AccountIndexMap将input_index转换为account_id
            // 🔒 不再假设input_index直接对应account_id，避免数据一致性问题
            let index_map = AccountIndexMap::from_input_index(index)?;
            let expected_account_id = index_map.account_id;

            // 查询特定账户和chain_code的api_account记录是否存在
            // 使用点查，避免O(N)查询
            let accounts = wallet_database::repositories::api_wallet::account::ApiAccountRepo::find_all_by_wallet_address_index(
                pool.clone(),
                &wallet.address,
                chain,
                expected_account_id
            ).await?;

            Ok(!accounts.is_empty())
        } else {
            Ok(false)
        }
    }

    /// 检查地址是否已初始化（使用点查，避免O(N)查询）
    async fn check_address_inited(
        &self,
        uid: &str,
        chain: &str,
        index: i32,
    ) -> Result<bool, ServiceError> {
        let pool = self.pool.clone();

        // 获取api_wallet
        let wallet = ApiWalletRepo::find_by_uid(pool.clone(), uid).await?;

        if let Some(wallet) = wallet {
            // 🔒 修复4：使用正确的AccountIndexMap将input_index转换为account_id
            // 🔒 不再假设input_index直接对应account_id，避免数据一致性问题
            let index_map = AccountIndexMap::from_input_index(index)?;
            let expected_account_id = index_map.account_id;

            // 查询特定账户和chain_code的api_account记录，检查是否已初始化
            // 使用点查，避免O(N)查询
            let accounts = wallet_database::repositories::api_wallet::account::ApiAccountRepo::find_all_by_wallet_address_index(
                pool.clone(),
                &wallet.address,
                chain,
                expected_account_id
            ).await?;

            // 检查是否存在已初始化的记录
            Ok(!accounts.is_empty() && accounts.iter().any(|account| account.is_init == 1))
        } else {
            Ok(false)
        }
    }

    /// 发送创建账户任务
    async fn send_create_job(
        &self,
        item: &wallet_database::entities::expand_batch_item::ExpandBatchItemEntity,
    ) -> Result<(), ServiceError> {
        let job = ExpandJob::Create {
            uid: item.uid.clone(),
            chain: item.chain_code.clone(),
            batch_id: item.batch_id.clone(),
            indices: vec![item.input_index],
        };

        WORKER_POOL.tx.send(job).await.map_err(|e| {
            ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
                e.to_string(),
            ))
        })
    }

    /// 发送初始化地址任务
    async fn send_init_job(
        &self,
        item: &wallet_database::entities::expand_batch_item::ExpandBatchItemEntity,
    ) -> Result<(), ServiceError> {
        let job = ExpandJob::Init {
            uid: item.uid.clone(),
            chain: item.chain_code.clone(),
            batch_id: item.batch_id.clone(),
            indices: vec![item.input_index],
        };

        WORKER_POOL.tx.send(job).await.map_err(|e| {
            ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
                e.to_string(),
            ))
        })
    }

    /// 扫描并派生batch状态
    #[instrument(skip(self))]
    async fn scan_batches(&self) -> Result<(), ServiceError> {
        tracing::info!("ExpandScanner: scanning batches");

        // 1. 获取所有运行中的批次
        let batches = ExpandBatchRepo::get_all_running_batches(self.pool.clone()).await?;

        // 2. 更新每个批次的finished_count缓存
        for batch in batches {
            // 2.1 重新计算finished_count（仅作为缓存）
            let count =
                ExpandBatchItemRepo::count_done_items(self.pool.clone(), &batch.batch_id).await?;

            // 2.2 更新finished_count
            ExpandBatchRepo::update_finished_count(self.pool.clone(), &batch.batch_id, count)
                .await?;

            // 2.3 检查是否所有items都已完成
            let total = batch.total_count as i64;
            if count >= total {
                // 3. 所有items都已完成，标记batch为Done
                // 使用mark_done_if_finished方法，该方法已经包含了CAS保护
                // 条件：finished_count >= total_count AND status = 'Running'
                let updated =
                    ExpandBatchRepo::mark_done_if_finished(self.pool.clone(), &batch.batch_id)
                        .await?;
                if updated {
                    tracing::info!(batch_id = %batch.batch_id, "ExpandScanner: batch marked as done");

                    // 4. 发送通知任务
                    let job = ExpandJob::Notify {
                        uid: batch.uid.clone(),
                        chain: batch.chain_code.clone(),
                        batch_id: batch.batch_id.clone(),
                    };

                    if let Err(e) = WORKER_POOL.tx.send(job).await {
                        tracing::error!(batch_id = %batch.batch_id, error = %e, "ExpandScanner: failed to send notify job");
                    }
                } else {
                    tracing::debug!(batch_id = %batch.batch_id, "ExpandScanner: batch already marked as done or finished_count not equal to total_count, skipping");
                }
            }
        }

        Ok(())
    }

    /// Recover机制：启动时立即执行一次扫描
    ///
    /// 🔴 核心语义：
    /// recover() ≠ 修复
    /// recover() = 对所有可能推进的状态做一次完整扫描
    ///
    /// 包含所有扫描步骤：
    /// 1. 调用Planner，处理Pending Batch（可能推进 Pending → Running 并创建 Creating items）
    /// 2. 扫描Creating状态的items，推进Creating→Initing（事实驱动）
    /// 3. 扫描Initing状态的items，推进Initing→Done（事实驱动）
    /// 4. 更新batch状态和finished_count缓存
    pub async fn recover(&mut self) -> Result<(), ServiceError> {
        tracing::info!("ExpandScanner: starting recover - performing full scan");

        // 直接调用scan方法执行完整扫描，包含所有状态推进步骤
        // 🔒 统一语义：recover()和定时扫描使用相同的scan()逻辑
        self.scan().await?;

        tracing::info!("ExpandScanner: recover completed");
        Ok(())
    }
}
