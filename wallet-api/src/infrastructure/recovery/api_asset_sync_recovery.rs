use std::{sync::Arc, time::Duration};

use tokio::time::sleep;
use wallet_database::repositories::api_wallet::api_asset_sync_state::ApiAssetSyncStateRepo;

use crate::{
    config::runtime_defaults, domain::api_wallet::assets::ApiAssetsDomain,
    error::service::ServiceError, infrastructure::recovery::pool::BackgroundTaskPool,
};

pub async fn start_api_asset_sync_worker(
    ctx: &'static crate::context::Context,
    background_task_pool: Arc<BackgroundTaskPool>,
) -> Result<(), ServiceError> {
    tracing::info!("启动 API 钱包资产同步恢复Worker");

    tokio::spawn(async move {
        crate::infrastructure::system_ready::wait_system_ready().await;
        tracing::info!("API 钱包资产同步恢复Worker检测到系统就绪，开始扫描可恢复任务");

        if let Err(e) = scan_and_dispatch(ctx, true, background_task_pool.clone()).await {
            tracing::error!("API 钱包资产同步恢复Worker启动扫描失败: {:?}", e);
        }

        loop {
            sleep(Duration::from_secs(5)).await;
            if let Err(e) = scan_and_dispatch(ctx, true, background_task_pool.clone()).await {
                tracing::error!("API 钱包资产同步恢复Worker扫描失败: {:?}", e);
            }
        }
    });

    Ok(())
}

async fn scan_and_dispatch(
    ctx: &'static crate::context::Context,
    include_stuck_running: bool,
    background_task_pool: Arc<BackgroundTaskPool>,
) -> Result<(), ServiceError> {
    let defaults = runtime_defaults::recovery();
    let api_pool = ctx.api_wallet_pool()?;

    let mut claimed = 0usize;
    loop {
        if claimed >= defaults.api_asset_sync_max_claims_per_round {
            tracing::debug!(
                claimed = claimed,
                metric = "api_asset_sync_claim_round_limit",
                "api asset sync recovery round reached claim limit"
            );
            break;
        }

        let task = ApiAssetSyncStateRepo::claim_next(&api_pool, include_stuck_running).await?;
        let Some(task) = task else { break };

        claimed += 1;
        let background_task_pool = background_task_pool.clone();
        background_task_pool
            .push(async move {
                process_one(ctx, task).await?;
                Ok(())
            })
            .await;
    }

    Ok(())
}

async fn process_one(
    ctx: &'static crate::context::Context,
    task: wallet_database::entities::api_asset_sync_state::ApiAssetSyncStateEntity,
) -> Result<(), ServiceError> {
    let api_pool = ctx.api_wallet_pool()?;
    let token_key = task.token_address.clone();

    tracing::info!(
        address = %task.address,
        chain_code = %task.chain_code,
        token_address = %token_key,
        attempt_count = task.attempt_count,
        "开始处理 API 钱包持久化资产同步任务"
    );

    let result = ApiAssetsDomain::sync_assets_by_addr_chain_durable_with_ctx(
        ctx,
        vec![task.address.clone()],
        Some(task.chain_code.clone()),
        token_key.clone(),
        task.attempt_count as u32,
    )
    .await;

    match result {
        Ok(()) => {
            ApiAssetSyncStateRepo::mark_done(&api_pool, &task.address, &task.chain_code, token_key)
                .await?;
            tracing::info!(
                address = %task.address,
                chain_code = %task.chain_code,
                "API 钱包持久化资产同步任务处理完成"
            );
        }
        Err(e) => {
            let msg = e.to_string();
            ApiAssetSyncStateRepo::mark_failed(
                &api_pool,
                &task.address,
                &task.chain_code,
                token_key,
                &msg,
            )
            .await?;
            tracing::warn!(
                address = %task.address,
                chain_code = %task.chain_code,
                error = %e,
                "API 钱包持久化资产同步任务处理失败"
            );
        }
    }

    Ok(())
}
