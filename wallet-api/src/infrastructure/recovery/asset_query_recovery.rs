use std::{sync::Arc, time::Duration};

use tokio::time::sleep;
use tracing::info;

use wallet_database::repositories::api_wallet::asset_query_state::AssetQueryStateRepo;

use crate::{
    config::runtime_defaults,
    error::service::ServiceError,
    infrastructure::{
        api_wallet_assets_sync::query_and_upsert_assets, recovery::pool::BackgroundTaskPool,
    },
};

pub async fn start_asset_query_worker(
    background_task_pool: Arc<BackgroundTaskPool>,
) -> Result<(), ServiceError> {
    info!("启动资产查询恢复Worker");

    tokio::spawn(async move {
        crate::infrastructure::system_ready::wait_system_ready().await;
        info!("资产查询恢复Worker检测到系统就绪，开始扫描可恢复任务");

        if let Err(e) = scan_and_dispatch(true, background_task_pool.clone()).await {
            tracing::error!("资产查询恢复Worker启动扫描失败: {:?}", e);
        }

        loop {
            sleep(Duration::from_secs(5)).await;
            if let Err(e) = scan_and_dispatch(true, background_task_pool.clone()).await {
                tracing::error!("资产查询恢复Worker扫描失败: {:?}", e);
            }
        }
    });

    Ok(())
}

async fn scan_and_dispatch(
    include_stuck_running: bool,
    background_task_pool: Arc<BackgroundTaskPool>,
) -> Result<(), ServiceError> {
    // 每轮认领上限用于平滑恢复流量，避免恢复线程自身制造请求尖峰。
    let defaults = runtime_defaults::recovery();
    let context = crate::context::CONTEXT.get().unwrap();
    let api_pool = context.api_wallet_pool()?;

    let mut claimed = 0usize;
    loop {
        if claimed >= defaults.asset_query_max_claims_per_round {
            tracing::debug!(
                claimed = claimed,
                metric = "asset_query_claim_round_limit",
                "asset query recovery round reached claim limit"
            );
            break;
        }

        let task = AssetQueryStateRepo::claim_next(&api_pool, include_stuck_running).await?;
        let Some(task) = task else { break };

        claimed += 1;
        let background_task_pool = background_task_pool.clone();
        background_task_pool
            .push(async move {
                process_one(task).await?;
                Ok(())
            })
            .await;
    }

    Ok(())
}

async fn process_one(
    task: wallet_database::entities::asset_query_state::AssetQueryStateEntity,
) -> Result<(), ServiceError> {
    let context = crate::context::CONTEXT.get().unwrap();
    let api_pool = context.api_wallet_pool()?;
    let backend = context.get_global_backend_api();

    let indices: Vec<i32> = match serde_json::from_str(&task.index_list_json) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!("invalid index_list_json: {e}");
            AssetQueryStateRepo::mark_failed(
                &api_pool,
                &task.uid,
                &task.chain_code,
                task.page,
                &msg,
            )
            .await?;
            return Ok(());
        }
    };

    if indices.is_empty() {
        AssetQueryStateRepo::mark_done(&api_pool, &task.uid, &task.chain_code, task.page).await?;
        return Ok(());
    }

    let req = wallet_transport_backend::request::api_wallet::address::AssetListReq::new(
        &task.uid,
        &task.chain_code,
        indices,
    );

    match query_and_upsert_assets(&api_pool, backend.as_ref(), &req).await {
        Ok(()) => {
            AssetQueryStateRepo::mark_done(&api_pool, &task.uid, &task.chain_code, task.page)
                .await?;
        }
        Err(e) => {
            let msg = e.to_string();
            AssetQueryStateRepo::mark_failed(
                &api_pool,
                &task.uid,
                &task.chain_code,
                task.page,
                &msg,
            )
            .await?;
        }
    }

    Ok(())
}
