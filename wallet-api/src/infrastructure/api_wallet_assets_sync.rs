use std::collections::HashSet;

use wallet_database::{
    ApiWalletDbPool,
    entities::{api_assets::ApiCreateAssetsVo, assets::AssetsId},
    repositories::api_wallet::{assets::ApiAssetsRepo, coin::ApiCoinRepo},
};
use wallet_transport_backend::{api::BackendApi, request::api_wallet::address::AssetListReq};

use crate::error::service::ServiceError;

pub(crate) async fn query_and_upsert_assets(
    api_pool: &ApiWalletDbPool,
    backend: &BackendApi,
    req: &AssetListReq,
) -> Result<(), ServiceError> {
    let list = backend.query_asset_list(req).await?;
    let default_coins_list = ApiCoinRepo::coin_list(api_pool).await?;

    let mut all_assets: Vec<ApiCreateAssetsVo> = Vec::new();
    let mut unique_addresses: HashSet<String> = HashSet::new();
    let mut unique_symbols: HashSet<String> = HashSet::new();

    for asset in list.0 {
        for address in asset.address_list {
            unique_addresses.insert(address.address.clone());
            for token in address.token_infos {
                unique_symbols.insert(token.symbol.clone());
                if let Some(coin) = default_coins_list.iter().find(|coin| {
                    coin.chain_code == req.chain_code
                        && coin.token_address.as_ref() == Some(&token.token_address)
                }) {
                    let assets_id = AssetsId::new(
                        &address.address,
                        &req.chain_code,
                        &token.symbol,
                        Some(token.token_address.clone()),
                    );

                    let balance_str = token.amount.to_string();

                    let assets =
                        ApiCreateAssetsVo::new(assets_id, coin.decimals, coin.protocol.clone(), 0)
                            .with_name(&coin.name)
                            .with_balance(&balance_str);

                    all_assets.push(assets);
                }
            }
        }
    }

    if !all_assets.is_empty() {
        ApiAssetsRepo::upsert_assets_multi_update_balance(api_pool, all_assets).await?;
    }

    // Best-effort: notify inner event to trigger further sync/calculation pipeline.
    let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
    if let Some(handles) = handles.upgrade() {
        let inner_event_handle = handles.get_global_inner_event_handle();
        let addr_list: Vec<String> = unique_addresses.into_iter().collect();
        let symbols: Vec<String> = unique_symbols.into_iter().collect();
        let data = crate::infrastructure::inner_event::SyncAssetsData::new(
            addr_list,
            req.chain_code.clone(),
            symbols,
            None,
        );
        if let Err(e) = inner_event_handle
            .send(crate::infrastructure::inner_event::InnerEvent::ApiWalletSyncAssets(data))
        {
            tracing::error!("发送资产同步事件失败: error={}", e);
        }
    }

    Ok(())
}
