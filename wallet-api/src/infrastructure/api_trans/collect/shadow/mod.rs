/// ============================================================================
///                            架构规则（必须遵守）
/// ============================================================================
///
/// 1. Shadow/Scanner模块禁止依赖任何legacy_* API
/// 2. 所有架构变更必须通过fact-based路径
/// 3. finished_at只能由mark_chain_finished写入
/// 4. 所有副作用必须基于已确认的链事实（transaction_time != NULL）
/// 5. 所有副作用必须有并发安全保障（DB约束或WHERE CAS）
/// 6. Scanner谓词只能基于事实字段，不能基于行为字段
///
/// ============================================================================
///                          Code Review Checklist
/// ============================================================================
///
/// - [ ] 是否调用legacy_* API？如果是，必须说明原因
/// - [ ] 是否写finished_at？如果不是mark_chain_finished，禁止
/// - [ ] 是否在Worker中引入外部副作用？如果是，必须是SideEffectWorker
/// - [ ] Scanner谓词是否只基于事实字段？
/// - [ ] 是否使用了基于行为的推断，而不是基于事实的判断？
/// - [ ] 所有操作是否有并发安全保障？
mod actor;
mod advancer;
mod dispatcher;
pub(crate) mod predicate;
mod scanner;
pub(crate) mod stage;
mod worker;

use std::sync::atomic::{AtomicBool, Ordering};

// Shadow系统开关，默认关闭
pub(crate) static COLLECT_SHADOW_ENABLED: AtomicBool = AtomicBool::new(false);

/// 注意：Confirm 不由 Shadow Worker 处理
///
/// 链上结果由 MQTT 注入，由 Domain 层落库，Shadow Worker 只负责：
/// - BuildTx：构建交易
/// - BroadcastTx：广播交易
///
/// Confirm 是 Domain 层对"外部事实注入"的处理，不是 Worker 的工作

/// 链事实轴意图
///
/// 🔒 规则：Confirm 是链事实轴的唯一终态
/// 🔒 规则：Build / Broadcast 是可回滚、可重试的中间态
/// 🔒 规则：只有 Confirm（transaction_time != NULL）才是"世界已发生"
#[derive(Debug, Clone)]
pub enum ChainIntent {
    /// 评估 TRON 资源闸门。
    ///
    /// 这是真正的“操作步骤”：
    /// - 读取链上资源与本地订单事实
    /// - 产出评估结果事实（resource_ready / need_platform_delegate）
    ///
    /// 注意：
    /// - `resource_ready` / `need_platform_delegate` 是评估结果状态，不是独立 intent
    /// - 后续 BuildTx / ExecuteResourceDelegation 仍由 scanner 基于事实继续推进
    /// - 资源链这次只补齐到“回到旧 collect 主链入口”为止
    /// - 子账户主币 / 出款地址补币 / 原失败收口继续沿用上一版已经闭环的旧流程
    EvalResourceGate(String),
    /// 构建交易
    BuildTx(String),
    /// 广播交易
    BroadcastTx(String),
    /// 恢复交易
    RecoverTx(String),
    /// 执行平台代理资源代理任务，trade_no 是资源任务号。
    ///
    /// 它虽然执行的是“资源动作”，但职责上仍属于 collect 主流程的链路步骤：
    /// - 目的不是形成一条与 collect 并列的独立主流程
    /// - 而是为 collect/withdraw 的后续推进补齐资源前置条件
    ///
    /// collect gate 的释放时机也要和这个边界区分开：
    /// - 成功释放由 `SendResourceResultAck` 收口
    /// - 失败放行由 `UploadResourceTxExecReceipt` 作为稳定失败收口点触发
    ExecuteResourceDelegation(String),
}

/// 副作用轴意图
///
/// 🔒 规则：所有副作用必须基于已确认的链事实（transaction_time != NULL）
/// 🔒 规则：所有副作用必须有并发安全保障（DB约束或WHERE CAS）
/// 🔒 规则：finished_at 表示链事实完成，不表示系统处理完成
///
/// SideEffectIntent MUST:
/// - depend only on persisted facts
/// - be safe to execute zero or many times
/// - never modify chain facts
#[derive(Debug, Clone)]
pub enum SideEffectIntent {
    /// 发送订单ACK
    /// SendOrderAck is a side-effect only, never handled by Shadow Worker
    SendOrderAck(String),
    /// 发送结果ACK
    /// SendResultAck is a side-effect only, never handled by Shadow Worker
    SendResultAck(String),
    /// 发送手续费结果确认ACK
    /// SendTxFeeResAck is a side-effect only, never handled by Shadow Worker
    ///
    /// ⚠️ 重要说明：
    /// - TxFeeResAck is a gate ACK, not a result ACK
    /// - It only unlocks further progression and carries no chain semantics
    /// - It exists solely in the "pre-broadcast" phase
    SendTxFeeResAck(String),
    /// 发送平台资源结果确认 ACK，trade_no 是资源任务号
    SendResourceResultAck(String),
    /// 发送平台资源任务接收 ACK，trade_no 是资源任务号
    SendResourceTaskAck(String),
    /// 上传平台代理资源执行回执，trade_no 是资源任务号
    UploadResourceTxExecReceipt(String),
    /// 上传服务费记录
    UploadServiceFee(String),
    /// 上传交易执行回执
    UploadTxExecReceipt(String),
}

/// 推进意图枚举
///
/// 表示Shadow Scanner生成的状态推进建议
#[derive(Debug, Clone)]
pub enum CollectIntent {
    /// 链事实轴意图
    Chain(ChainIntent),
    /// 副作用轴意图
    SideEffect(SideEffectIntent),
}
// 重新导出内部模块的类型，方便外部使用
pub use actor::CollectorShadowActorSystem;
pub use advancer::ShadowAdvancer;
pub use dispatcher::DispatcherConfig;
pub(crate) use dispatcher::ShadowDispatcher;
pub use scanner::{ScannerConfig, ShadowScanner};
pub use worker::{ShadowCollectCommand, ShadowCollectWorker, SideEffectCommand, SideEffectWorker};

/// Shadow系统初始化
pub(crate) async fn init(
    ctx: &'static crate::context::Context,
) -> Option<actor::CollectorShadowActorSystem> {
    // 检查开关是否开启
    if !COLLECT_SHADOW_ENABLED.load(Ordering::Relaxed) {
        tracing::info!("Collect Shadow System is disabled");
        return None;
    }

    // 初始化Shadow Actor系统
    let actor_system = match actor::CollectorShadowActorSystem::new(ctx) {
        Ok(actor_system) => actor_system,
        Err(error) => {
            tracing::error!(?error, "Collect Shadow System failed to initialize");
            return None;
        }
    };

    tracing::info!("Collect Shadow System initialized and started");
    Some(actor_system)
}

/// 启用Shadow系统
pub fn enable() {
    COLLECT_SHADOW_ENABLED.store(true, Ordering::Relaxed);
    tracing::info!("Collect Shadow System enabled");
}

/// 禁用Shadow系统
pub fn disable() {
    COLLECT_SHADOW_ENABLED.store(false, Ordering::Relaxed);
    tracing::info!("Collect Shadow System disabled");
}

/// 检查Shadow系统是否启用
pub fn is_enabled() -> bool {
    COLLECT_SHADOW_ENABLED.load(Ordering::Relaxed)
}
