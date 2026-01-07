// bootstrap.rs
use crate::error::service::ServiceError;
use futures::stream::{FuturesUnordered, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::{sync::Semaphore, time::sleep};
use tracing::info;
use wallet_database::DbPool;
use wallet_transport_backend::{
    api::BackendApi,
    request::api_wallet::address::{AddressListReq, AssetListReq},
};

/// 地址恢复主流程
pub async fn bootstrap_address_recovery(
    uid: String,
    chain_code: String,
    backend: Arc<BackendApi>,
    pool: DbPool,
) -> Result<(), ServiceError> {
    info!("开始地址恢复流程: uid={}, chain_code={}", uid, chain_code);

    let sem = Arc::new(Semaphore::new(16));
    let mut tasks = FuturesUnordered::new();
    let mut has_more = true;
    let mut page = 0;

    // 分页拉取地址
    while has_more {
        info!("拉取地址第 {} 页: uid={}, chain_code={}", page, uid, chain_code);
        let res = backend
            .query_used_address_list(&AddressListReq::new(&uid, &chain_code, page, 500))
            .await?;

        let addresses = res.content;
        if addresses.is_empty() {
            has_more = false;
            info!("地址拉取完成: uid={}, chain_code={}", uid, chain_code);
            break;
        }

        // 优先插入地址到本地
        batch_insert_addresses(&pool, &uid, &chain_code, &addresses).await?;

        // 提交后台任务
        for address in addresses {
            let permit = sem.clone().acquire_owned().await.map_err(|e| {
                ServiceError::System(crate::error::system::SystemError::Service(e.to_string()))
            })?;
            let backend_clone = backend.clone();
            let pool_clone = pool.clone();
            let uid_clone = uid.clone();
            let chain_code_clone = chain_code.clone();

            tasks.push(async move {
                let _keep = permit;
                let res = crate::infrastructure::recovery::sync_address::sync_address(
                    &uid_clone,
                    &chain_code_clone,
                    &address,
                    backend_clone,
                    pool_clone,
                )
                .await;

                // 错误处理：记录日志，不阻断流程
                if let Err(e) = res {
                    tracing::warn!("sync failed for index {}: {:?}", address.index, e);
                }
            });
        }

        page += 1;
    }

    // 排空后台任务
    info!("等待所有后台同步任务完成: uid={}, chain_code={}", uid, chain_code);
    while let Some(_) = tasks.next().await {}

    // 30秒后启动补扫，确保最终一致性
    let uid_clone = uid.clone();
    let chain_code_clone = chain_code.clone();
    let pool_clone = pool.clone();
    info!("安排30秒后补扫任务: uid={}, chain_code={}", uid, chain_code);
    tokio::spawn(async move {
        sleep(Duration::from_secs(30)).await;
        if let Err(e) = crate::infrastructure::recovery::rescan::rescan_missing_addresses(
            &uid_clone,
            &chain_code_clone,
            pool_clone,
        )
        .await
        {
            tracing::warn!("Rescan failed: {:?}", e);
        }
    });

    info!("地址恢复主流程完成: uid={}, chain_code={}", uid, chain_code);
    Ok(())
}

/// 批量插入地址到本地数据库
async fn batch_insert_addresses(
    pool: &DbPool,
    uid: &str,
    chain_code: &str,
    addresses: &[wallet_transport_backend::response_vo::api_wallet::address::UsedAddressItem],
) -> Result<(), ServiceError> {
    // 这里需要实现批量插入逻辑，暂时留空
    // 实际实现中应该调用ApiAccountDomain或ApiAccountRepo的批量插入方法
    Ok(())
}
