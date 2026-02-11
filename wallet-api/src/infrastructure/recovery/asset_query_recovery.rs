use std::{sync::Arc, time::Duration};

use tokio::time::sleep;
use tracing::info;

use wallet_database::repositories::api_wallet::asset_query_state::AssetQueryStateRepo;

use crate::{
    error::service::ServiceError,
    infrastructure::{
        api_wallet_assets_sync::query_and_upsert_assets, recovery::pool::BackgroundTaskPool,
    },
};

const MAX_CLAIMS_PER_ROUND: usize = 50;

pub async fn start_asset_query_worker(
    background_task_pool: Arc<BackgroundTaskPool>,
) -> Result<(), ServiceError> {
    info!("启动资产查询恢复Worker");

    // Startup scan: include stuck running tasks.
    scan_and_dispatch(true, background_task_pool.clone()).await?;

    tokio::spawn(async move {
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
    let context = crate::context::CONTEXT.get().unwrap();
    let api_pool = context.api_wallet_pool()?;

    let mut claimed = 0usize;
    loop {
        if claimed >= MAX_CLAIMS_PER_ROUND {
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
