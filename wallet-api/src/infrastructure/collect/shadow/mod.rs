mod actor;
mod dispatcher;
mod scanner;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use sqlx::SqlitePool;
use tokio::sync::mpsc;

// Shadow系统开关，默认关闭
pub(crate) static COLLECT_SHADOW_ENABLED: AtomicBool = AtomicBool::new(false);

/// 推进意图枚举
///
/// 表示Shadow Scanner生成的状态推进建议
#[derive(Debug, Clone)]
pub enum CollectIntent {
    /// 构建交易
    BuildTx(String),
    /// 广播交易
    Broadcast(String),
    /// 确认交易
    Confirm(String),
    /// 上报ACK
    Ack(String),
}

// 重新导出内部模块的类型，方便外部使用
pub use actor::{CollectorShadowActorSystem, DispatcherActorMessage};
pub use dispatcher::DispatcherConfig;
pub use scanner::{ScannerConfig, ShadowScanner};
use wallet_database::CollectDbPool;

/// Shadow系统初始化
pub(crate) async fn init(
    pool: CollectDbPool,
    tx_tx: mpsc::Sender<crate::infrastructure::collect::command::ProcessCollectTxCommand>,
    report_tx: mpsc::Sender<crate::infrastructure::collect::command::ProcessCollectTxReportCommand>,
    confirm_report_tx: mpsc::Sender<
        crate::infrastructure::collect::command::ProcessCollectTxConfirmReportCommand,
    >,
) -> Option<actor::CollectorShadowActorSystem> {
    // 检查开关是否开启
    if !COLLECT_SHADOW_ENABLED.load(Ordering::Relaxed) {
        tracing::info!("Collect Shadow System is disabled");
        return None;
    }

    // 初始化Shadow Actor系统
    let actor_system =
        actor::CollectorShadowActorSystem::new(pool, tx_tx, report_tx, confirm_report_tx);

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
