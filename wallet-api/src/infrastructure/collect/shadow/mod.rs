mod actor;
mod dispatcher;
mod scanner;
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

/// 推进意图枚举
///
/// 表示Shadow Scanner生成的状态推进建议
#[derive(Debug, Clone)]
pub enum CollectIntent {
    /// 构建交易
    BuildTx(String),
    /// 广播交易
    Broadcast(String),
}
// 重新导出内部模块的类型，方便外部使用
pub use actor::CollectorShadowActorSystem;
pub use dispatcher::DispatcherConfig;
pub use scanner::{ScannerConfig, ShadowScanner};
use wallet_database::{CollectDbPool, CoreDbPool};

/// Shadow系统初始化
pub(crate) async fn init(
    api_funds_pool: CollectDbPool,
    core_pool: CoreDbPool,
) -> Option<actor::CollectorShadowActorSystem> {
    // 检查开关是否开启
    if !COLLECT_SHADOW_ENABLED.load(Ordering::Relaxed) {
        tracing::info!("Collect Shadow System is disabled");
        return None;
    }

    // 初始化Shadow Actor系统
    let actor_system = actor::CollectorShadowActorSystem::new(api_funds_pool, core_pool);

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
