use std::{sync::Arc, time::Duration};
use tokio::time::sleep;
use tracing::info;

use wallet_database::{
    entities::address_query_state::{AddressQueryStateEntity, AddressQueryStatus},
    repositories::api_wallet::address_query_state::AddressQueryStateRepo,
};

use crate::{error::service::ServiceError, infrastructure::recovery::pool::BackgroundTaskPool};

/// 启动地址恢复Worker
/// 1. 启动时扫描一次 Running + Failed 任务
/// 2. 然后每5秒扫描一次 Failed 任务
pub async fn start_address_recover_worker(
    background_task_pool: Arc<BackgroundTaskPool>,
) -> Result<(), ServiceError> {
    info!("启动地址恢复Worker");

    // 启动时恢复未完成任务 (Running + Failed)
    scan_and_dispatch(true, background_task_pool.clone()).await?;

    // 后台定时扫描（只扫Failed）
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(5)).await;
            if let Err(e) = scan_and_dispatch(false, background_task_pool.clone()).await {
                tracing::error!("地址恢复Worker扫描失败: {:?}", e);
            }
        }
    });

    Ok(())
}

/// 扫描并分发地址恢复任务
/// - is_startup: true时扫描Running + Failed，false时只扫描Failed
pub async fn scan_and_dispatch(
    is_startup: bool,
    background_task_pool: Arc<BackgroundTaskPool>,
) -> Result<(), ServiceError> {
    info!(is_startup = is_startup, "开始扫描地址查询状态");

    // 获取全局上下文和数据库连接池
    let context = crate::context::CONTEXT.get().unwrap();
    let pool = context.get_global_sqlite_pool()?;

    // 根据is_startup决定查询条件
    let query_states = if is_startup {
        // 启动时：查询Running + Failed
        let running_states =
            AddressQueryStateRepo::list_by_status(&pool, AddressQueryStatus::Running).await?;
        let failed_states =
            AddressQueryStateRepo::list_by_status(&pool, AddressQueryStatus::Failed).await?;

        let mut all_states: Vec<AddressQueryStateEntity> = Vec::new();
        all_states.extend(running_states);
        all_states.extend(failed_states);
        all_states
    } else {
        // 运行中：查询Failed + 长时间未更新的Running（10分钟）
        AddressQueryStateRepo::list_recoverable_tasks(&pool, true).await?
    };

    info!(is_startup = is_startup, total = query_states.len(), "扫描到待处理的地址查询状态数量");

    // 处理每个状态
    for state in query_states {
        let state_clone = state.clone();

        // 将任务推送到现有的BackgroundTaskPool
        background_task_pool.push(async move {
            info!(uid = %state_clone.uid, chain_code = %state_clone.chain_code, "开始处理地址恢复任务");
            // 使用ApiAccountDomain的continue_recover方法继续恢复
            let res = crate::domain::api_wallet::account::ApiAccountDomain::continue_recover(&state_clone).await;
            if let Err(e) = res {
                tracing::error!(uid = %state_clone.uid, chain_code = %state_clone.chain_code, "地址恢复任务失败: {:?}", e);
            } else {
                tracing::debug!(uid = %state_clone.uid, chain_code = %state_clone.chain_code, "地址恢复任务完成");
            }
            Ok(())
        }).await;
    }

    Ok(())
}
