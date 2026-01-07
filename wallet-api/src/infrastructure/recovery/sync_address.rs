// sync_address.rs
use crate::error::service::ServiceError;
use std::sync::Arc;
use tracing::{info, warn};
use wallet_database::DbPool;
use wallet_transport_backend::{api::BackendApi, request::api_wallet::address::AssetListReq};

/// 单地址同步逻辑
pub async fn sync_address(
    uid: &str,
    chain_code: &str,
    address: &wallet_transport_backend::response_vo::api_wallet::address::UsedAddressItem,
    backend: Arc<BackendApi>,
    pool: DbPool,
) -> Result<(), ServiceError> {
    info!("开始同步地址信息: uid={}, chain_code={}, index={}", uid, chain_code, address.index);

    // 1. 拉取余额
    let balance_result =
        backend.query_asset_list(&AssetListReq::new(uid, chain_code, vec![address.index])).await;

    match balance_result {
        Ok(balance) => {
            // 2. 更新本地数据库
            if let Err(e) = update_address_info(&pool, address, &balance).await {
                warn!(
                    "更新地址信息失败: uid={}, chain_code={}, index={}, error={:?}",
                    uid, chain_code, address.index, e
                );
                return Err(e);
            }
            info!(
                "地址信息同步完成: uid={}, chain_code={}, index={}",
                uid, chain_code, address.index
            );
        }
        Err(e) => {
            warn!(
                "拉取余额失败: uid={}, chain_code={}, index={}, error={:?}",
                uid, chain_code, address.index, e
            );
            return Err(ServiceError::System(crate::error::system::SystemError::Service(
                e.to_string(),
            )));
        }
    }

    Ok(())
}

/// 更新地址信息到本地数据库
async fn update_address_info(
    pool: &DbPool,
    address: &wallet_transport_backend::response_vo::api_wallet::address::UsedAddressItem,
    balance: &wallet_transport_backend::response_vo::api_wallet::address::AssetsListRes,
) -> Result<(), ServiceError> {
    // 这里需要实现更新地址信息的逻辑，暂时留空
    // 实际实现中应该调用相关的Repo来更新地址的余额、nonce等信息
    Ok(())
}
