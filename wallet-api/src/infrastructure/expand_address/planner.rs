// planner.rs
use std::sync::Arc;

use sqlx::SqlitePool;
use tracing::instrument;
use wallet_database::entities::{
    address_query_state::AddressQueryStatus, expand_batch::ExpandBatchStatus,
};

use crate::{
    domain::api_wallet::account::ApiAccountDomain, error::service::ServiceError,
    infrastructure::expand_address::event::ExpandEventSender,
};
use wallet_database::repositories::api_wallet::{
    address_query_state::AddressQueryStateRepo, expand_batch::ExpandBatchRepo,
    expand_batch_item::ExpandBatchItemRepo,
};

/// ExpandPlanner - 负责将Pending状态的Batch转换为Running状态，并一次性创建所有Item
///
/// 🔴 核心职责：
/// - 将Pending状态的Batch转换为Running状态
/// - 一次性创建所有Item，确保扩容边界不被后续地址查询污染
/// - 确保在并发/crash/多实例环境下的正确性
/// - 发送HintScan事件通知Scanner检查新创建的Item
///
/// 🔴 核心幂等边界声明：
/// - **可重复执行**：多次调用不会产生副作用
/// - **可并发执行**：多实例同时运行不会导致数据不一致
/// - **可在任意时刻执行**：无时间依赖，可随时触发
/// - **纯创建逻辑**：仅创建Item，不修改已存在item的状态
///
/// 🔴 核心操作约束：
/// 1. **只允许CREATE**：禁止UPDATE操作，已存在的items状态不会被改变
/// 2. **唯一性保证**：基于(uid, chain, input_index)唯一索引，冲突视为已存在
/// 3. **一次性创建**：Batch从Pending→Running时一次性创建所有Item
/// 4. **触发时机**：仅由Scanner或定时任务触发，不主动运行
/// 5. **幂等性实现**：通过唯一索引和INSERT IGNORE(或ON DUPLICATE KEY)机制保证
/// 6. **CAS保护**：使用数据库级CAS确保Batch状态转换的原子性
/// 7. **事件驱动**：创建Item后发送HintScan事件，提高系统响应性
///
/// 🔴 设计意图：
/// - Planner = 延迟决策工具，确保扩容边界在安全时机冻结
/// - 解决"扩容请求可能早于地址事实收敛"的并发问题
/// - 确保系统在各种故障场景下的可恢复性
/// - 支持水平扩展，可部署多个实例
/// - 简化错误处理，失败可直接重试
/// - 事件驱动提高响应性，定时兜底保证可靠性
#[derive(Clone)]
pub struct ExpandPlanner {
    pool: Arc<SqlitePool>,
    event_tx: Option<ExpandEventSender>,
}

impl ExpandPlanner {
    pub fn new(pool: Arc<SqlitePool>, event_tx: Option<ExpandEventSender>) -> Self {
        Self { pool, event_tx }
    }

    /// 🔒 显式门控函数：can_plan - 明确真值表，不可被绕过
    /// 🔒 设计真值表：
    /// | address_query_state | can_plan | 原因          |
    /// | ------------------- | -------- | ----------- |
    /// | Some(Running)       | ❌ false  | 边界未收敛       |
    /// | Some(Done)          | ✅ true   | 边界已稳定       |
    /// | Some(Failed)        | ❌ false  | 数据不可信       |
    /// | **None**            | ✅ true   | 不需要 / 不会有查询 |
    /// 🔒 核心语义：
    /// - `None` 表示该 uid + chain 从设计上就不需要地址查询
    /// - 绝不表示"查询尚未开始"或"查询被遗漏"
    /// - `None` is a design-time guarantee, not a runtime missing state
    fn can_plan(
        &self,
        query_state: Option<
            wallet_database::entities::address_query_state::AddressQueryStateEntity,
        >,
        batch_id: &str,
    ) -> bool {
        match query_state {
            Some(state) => {
                match state.status {
                    AddressQueryStatus::Running => {
                        // 查询正在进行中，不允许创建Item
                        tracing::info!(batch_id = %batch_id, "ExpandPlanner: address query is still running, skipping - wait for query completion");
                        false
                    }
                    AddressQueryStatus::Done => {
                        // 查询完成，可以创建Item
                        tracing::info!(batch_id = %batch_id, "ExpandPlanner: address query done, proceeding to create items");
                        true
                    }
                    AddressQueryStatus::Failed => {
                        // 查询失败，不允许创建Item
                        tracing::error!(batch_id = %batch_id, "ExpandPlanner: address query failed, skipping permanently - manual intervention required");
                        false
                    }
                }
            }
            None => {
                // 🔒 系统级语义约束：None 是设计时保证，不是运行时缺失状态
                // 🔒 唯一合法含义：该 uid + chain 从设计上就不需要地址查询
                // 🔒 反例注释：
                // 🔒 ❌ 错误：None = "还没写入 address_query_state"
                // 🔒 ❌ 错误：None = "查询尚未开始"
                // 🔒 ❌ 错误：None = "查询被遗漏"
                // 🔒 ✅ 正确：None = "该链从设计上就不需要地址查询"
                // 🔒 若未来该链新增查询逻辑，AddressQueryState 必须显式写入 Running / Done
                // 🔒 明确禁止将 None 解释为任何运行时状态
                tracing::info!(batch_id = %batch_id, "ExpandPlanner: address query state not found, treating as DONE - this is a design-time guarantee");
                true
            }
        }
    }

    /// 处理所有Pending状态的批次，将它们转换为Running状态并创建Item
    ///
    /// 🔒 工程级最终注释模板
    /// plan_all_batches 只是调度入口，不包含任何业务判断
    /// 所有决策必须发生在 plan_batch 内
    /// 🔒 明确禁止在该函数中添加任何条件判断
    /// 🔒 所有业务判断必须且只能存在于 plan_batch() 函数中
    /// 🔒 该函数的唯一作用是遍历Pending批次并调用plan_batch()
    /// 🔒 禁止修改该函数的核心逻辑，禁止添加任何业务假设
    // #[instrument(skip(self))]
    pub async fn plan_all_batches(&self) -> Result<(), ServiceError> {
        tracing::info!("ExpandPlanner: planning all batches");

        // 获取所有Pending状态的批次
        let pending_batches =
            ExpandBatchRepo::get_by_status(self.pool.clone(), ExpandBatchStatus::Pending).await?;

        for batch in pending_batches {
            tracing::info!(batch_id = %batch.batch_id, status = ?batch.status, "ExpandPlanner: processing pending batch");

            // 处理单个批次，所有业务判断由plan_batch()完成
            self.plan_batch(&batch.batch_id).await?;
        }

        Ok(())
    }

    /// 处理单个Pending状态的批次，将其转换为Running状态并创建所有Item
    // #[instrument(skip(self))]
    pub async fn plan_batch(&self, batch_id: &str) -> Result<(), ServiceError> {
        tracing::info!(batch_id = %batch_id, "ExpandPlanner: planning batch items");

        // 获取批次信息，确保状态为Pending
        let batch = ExpandBatchRepo::get_batch(self.pool.clone(), batch_id).await?;

        if let Some(batch) = batch {
            // 检查Batch状态是否为Pending
            if batch.status != ExpandBatchStatus::Pending {
                tracing::info!(batch_id = %batch_id, status = ?batch.status, "ExpandPlanner: batch not in pending state, skipping");
                return Ok(());
            }

            // 检查地址查询是否完成
            // 🔒 核心约束：扩容边界冻结必须晚于地址查询完成
            // 🔒 含义：Batch可以提前创建，但不能提前Running
            let query_state = AddressQueryStateRepo::get_by_uid_and_chain(
                &self.pool,
                &batch.uid,
                &batch.chain_code,
            )
            .await?;

            // 🔒 显式门控函数：can_plan - 明确真值表，不可被绕过
            // 🔒 这是把"设计真值表"变成"代码护城河"
            // 🔒 防止未来某人绕过address_query_state
            let can_plan = self.can_plan(query_state, batch_id);
            if !can_plan {
                return Ok(());
            }

            // 使用CAS将Batch状态从Pending转为Running，确保只有一个Planner实例能成功
            // 🔒 核心语义：Planner是唯一不可逆决策者，只有赢CAS的实例才能看到世界
            // 🔒 顺序重要性：CAS之前不能读世界，否则会破坏冻结点语义
            let won = ExpandBatchRepo::mark_running_if_pending(self.pool.clone(), batch_id).await?;
            if !won {
                tracing::info!(batch_id = %batch_id, "ExpandPlanner: batch already processed by another instance");
                return Ok(());
            }

            // 🔒 补充修订A：CAS成功后立刻再读一次Batch，确保状态正确
            // 🔒 这是抗未来修改的重要保障，防止CAS后Batch状态被意外修改
            let updated_batch = ExpandBatchRepo::get_batch(self.pool.clone(), batch_id).await?;
            if let Some(updated_batch) = updated_batch {
                // 🔒 系统级不变量：Planner必须只创建一次items，batch_item_count必须为0
                // 🔒 这是invariant violation，不是业务分支
                // 🔒 确保在release模式下也能检测到invariant violation
                // 🔒 明确这是不可恢复的错误，必须返回错误而不是悄悄处理
                let batch_item_count =
                    ExpandBatchItemRepo::count_by_batch_id(self.pool.clone(), batch_id).await?;
                if batch_item_count != 0 {
                    tracing::error!(
                        batch_id = %batch_id,
                        batch_item_count = batch_item_count,
                        "Planner invariant violation: batch items already exist, must only create items once"
                    );
                    return Err(ServiceError::System(
                    crate::error::system::SystemError::Internal(
                        "Planner invariant violation: batch items already exist, must only create items once".to_string()
                    )
                ));
                }

                // 🔒 扩容index在Pending→Running时一次性冻结
                // 🔒 使用准确的索引分配算法，确保不回退、不重复、不要求连续
                // 🔒 一次性创建所有items，确保扩容数量符合用户期望
                // 🔒 Item创建时直接为Creating状态，不再有Pending状态的Item
                // 🔒 只在address_query_state为Done或None时执行，确保边界稳定
                let indices = ApiAccountDomain::calculate_indices_for_expansion(
                    &updated_batch.uid,
                    &updated_batch.chain_code,
                    batch_id,
                    updated_batch.total_count as u32,
                )
                .await?;

                // 只有赢CAS的实例才能创建Item
                ExpandBatchItemRepo::batch_create_items(
                    self.pool.clone(),
                    &updated_batch.uid,
                    batch_id,
                    &updated_batch.chain_code,
                    &indices,
                )
                .await?;

                let created = indices.len() as i64;
                tracing::info!(batch_id = %batch_id, created = created, "ExpandPlanner: batch planned and moved to running");

                // 如果创建了至少1条item，发送HintScan事件
                if created > 0 {
                    if let Some(tx) = &self.event_tx {
                        // best-effort hint, ignore failure
                        let _ = tx
                            .send(
                                crate::infrastructure::expand_address::event::ExpandEvent::HintScan,
                            )
                            .await;
                    }
                }
            } else {
                // 理论上不应该发生，因为CAS刚刚成功
                tracing::error!(batch_id = %batch_id, "ExpandPlanner: batch not found after successful CAS, this should not happen");
                return Ok(());
            }
        }

        Ok(())
    }
}
