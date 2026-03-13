use std::{collections::HashSet, time::Instant};

use once_cell::sync::Lazy;
use tokio::sync::Semaphore;
use wallet_database::{
    ApiWalletDbPool,
    entities::{api_assets::ApiCreateAssetsVo, asset_token_key::AssetTokenKey, assets::AssetsId},
    repositories::api_wallet::{assets::ApiAssetsRepo, coin::ApiCoinRepo},
};
use wallet_transport_backend::{api::BackendApi, request::api_wallet::address::AssetListReq};

use crate::error::{service::ServiceError, system::SystemError};

const DEFAULT_ASSET_UPSERT_MAX_CONCURRENT: usize = 2;

fn read_asset_upsert_max_concurrent() -> usize {
    std::env::var("API_ASSET_UPSERT_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|v| v.clamp(1, 8))
        .unwrap_or(DEFAULT_ASSET_UPSERT_MAX_CONCURRENT)
}

static ASSET_UPSERT_SEMAPHORE: Lazy<Semaphore> =
    Lazy::new(|| Semaphore::new(read_asset_upsert_max_concurrent()));

pub(crate) async fn query_and_upsert_assets(
    api_pool: &ApiWalletDbPool,
    backend: &BackendApi,
    req: &AssetListReq,
) -> Result<(), ServiceError> {
    let list = backend.query_asset_list(req).await?;
    let default_coins_list = ApiCoinRepo::coin_list(api_pool).await?;

    let mut all_assets: Vec<ApiCreateAssetsVo> = Vec::new();
    let mut unique_addresses: HashSet<String> = HashSet::new();

    for asset in list.0 {
        for address in asset.address_list {
            unique_addresses.insert(address.address.clone());
            for token in address.token_infos {
                if let Some(coin) = default_coins_list.iter().find(|coin| {
                    coin.chain_code == req.chain_code
                        && coin.token_address.as_db_str() == token.token_address.as_str()
                }) {
                    let assets_id = AssetsId::new(
                        &address.address,
                        &req.chain_code,
                        Some(token.token_address.clone()).into(),
                    );

                    let balance_str = token.amount.to_string();

                    let assets = ApiCreateAssetsVo::new(
                        assets_id,
                        &token.symbol.to_ascii_uppercase(),
                        coin.decimals,
                        coin.protocol.clone(),
                        0,
                    )
                    .with_name(&coin.name)
                    .with_balance(&balance_str);

                    all_assets.push(assets);
                }
            }
        }
    }

    if !all_assets.is_empty() {
        // Limit concurrent bulk upsert writers to avoid exhausting the shared sqlite pool.
        let gate_wait_start = Instant::now();
        let _permit = ASSET_UPSERT_SEMAPHORE.acquire().await.map_err(|_| {
            ServiceError::System(SystemError::Internal("asset upsert semaphore closed".to_string()))
        })?;
        let wait_ms = gate_wait_start.elapsed().as_millis();
        if wait_ms > 500 {
            tracing::warn!(
                wait_ms = wait_ms,
                chain_code = %req.chain_code,
                asset_count = all_assets.len(),
                "asset upsert gate wait too long"
            );
        }
        ApiAssetsRepo::upsert_assets_multi_update_balance(api_pool, all_assets).await?;
    }

    // Best-effort: notify inner event to trigger further sync/calculation pipeline.
    let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
    if let Some(handles) = handles.upgrade() {
        let inner_event_handle = handles.get_global_inner_event_handle();
        let addr_list: Vec<String> = unique_addresses.into_iter().collect();
        let data = crate::infrastructure::inner_event::SyncAssetsData::new_with_token_key(
            addr_list,
            req.chain_code.clone(),
            AssetTokenKey::Native,
        );
        if let Err(e) = inner_event_handle
            .send(crate::infrastructure::inner_event::InnerEvent::ApiWalletSyncAssets(data))
        {
            tracing::error!("发送资产同步事件失败: error={}", e);
        }
    }

    Ok(())
}
