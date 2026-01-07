// rescan.rs
use crate::error::service::ServiceError;
use futures::stream::{FuturesUnordered, StreamExt};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn};
use wallet_database::DbPool;

/// 补扫缺失信息的地址
pub async fn rescan_missing_addresses(
    uid: &str,
    chain_code: &str,
    pool: DbPool,
) -> Result<(), ServiceError> {
    info!("开始补扫缺失信息的地址: uid={}, chain_code={}", uid, chain_code);

    // 1. 查询本地缺失信息的地址
    let missing_addresses = find_missing_info_addresses(&pool, uid, chain_code).await?;

    if missing_addresses.is_empty() {
        info!("没有需要补扫的地址: uid={}, chain_code={}", uid, chain_code);
        return Ok(());
    }

    info!(
        "发现需要补扫的地址数量: uid={}, chain_code={}, count={}",
        uid,
        chain_code,
        missing_addresses.len()
    );

    // 2. 补扫这些地址
    let sem = Arc::new(Semaphore::new(16));
    let mut tasks = FuturesUnordered::new();

    for address in missing_addresses {
        let permit = sem.clone().acquire_owned().await.map_err(|e| {
            ServiceError::System(crate::error::system::SystemError::Service(e.to_string()))
        })?;
        let pool_clone = pool.clone();
        let uid_clone = uid.to_string();
        let chain_code_clone = chain_code.to_string();

        tasks.push(async move {
            let _keep = permit;
            if let Err(e) =
                rescan_single_address(&uid_clone, &chain_code_clone, &address, &pool_clone).await
            {
                warn!(
                    "补扫地址失败: uid={}, chain_code={}, address={}, error={:?}",
                    uid_clone, chain_code_clone, address, e
                );
            }
        });
    }

    // 等待所有补扫任务完成
    while let Some(_) = tasks.next().await {}

    info!("地址补扫完成: uid={}, chain_code={}", uid, chain_code);
    Ok(())
}

/// 查找缺失信息的地址
async fn find_missing_info_addresses(
    pool: &DbPool,
    uid: &str,
    chain_code: &str,
) -> Result<Vec<String>, ServiceError> {
    // 这里需要实现查询逻辑，暂时返回空列表
    // 实际实现中应该查询那些缺失余额、nonce等信息的地址
    Ok(Vec::new())
}

/// 补扫单个地址
async fn rescan_single_address(
    uid: &str,
    chain_code: &str,
    address: &str,
    pool: &DbPool,
) -> Result<(), ServiceError> {
    // 这里需要实现单个地址的补扫逻辑
    // 实际实现中应该重新拉取该地址的所有信息并更新本地数据库
    Ok(())
}
