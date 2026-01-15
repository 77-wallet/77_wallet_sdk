// expand_init/executor.rs

use crate::{domain::app::config::ConfigDomain, error::service::ServiceError};
use std::collections::HashMap;
use wallet_database::repositories::api_wallet::{account::ApiAccountRepo, wallet::ApiWalletRepo};
use wallet_transport_backend::request::api_wallet::address::ApiAddressInitReq;
/// 异步执行INIT任务
/// 执行逻辑：
/// 1. 检查keys_reset_status
/// 2. 调用backend.expand_address接口
/// 3. 更新数据库中account的init状态
/// 4. 记录执行结果
/// 参数：init_req - INIT请求
/// 返回：Result<(), ServiceError> - 执行结果
pub async fn do_init(init_req: ApiAddressInitReq) -> Result<(), ServiceError> {
    tracing::info!(
        batch_id = ?init_req.batch_id,
        address_count = init_req.address_list.0.len(),
        "INIT_EXECUTOR: starting init task"
    );

    // 1. 检查keys_reset_status
    let status = ConfigDomain::get_keys_reset_status().await?;
    if let Some(status) = status
        && let Some(false) = status.status
    {
        tracing::error!("INIT_EXECUTOR: Keys not reset, cannot process address init request");
        return Err(crate::error::business::BusinessError::Config(
            crate::error::business::config::ConfigError::KeysNotReset,
        )
        .into());
    }

    // 2. 获取backend实例并调用expand_address接口
    let backend = crate::context::get_context()?.get_global_backend_api();

    tracing::info!(
        "INIT_EXECUTOR: calling backend.expand_address, address_count={}, batch_id={:?}",
        init_req.address_list.0.len(),
        init_req.batch_id
    );

    let result = backend.expand_address(&init_req).await;

    match result {
        Ok(_) => {
            tracing::info!(
                "INIT_EXECUTOR: backend.expand_address completed successfully, address_count={}",
                init_req.address_list.0.len()
            );

            // 3. 更新数据库中account的init状态
            let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

            tracing::info!("INIT_EXECUTOR: starting database operations for address init");

            for address in init_req.address_list.0.iter() {
                tracing::info!(
                    "INIT_EXECUTOR: processing address: uid={}, chain_code={}, index={}, address={}",
                    address.uid,
                    address.chain_code,
                    address.index,
                    address.address
                );

                let wallet = ApiWalletRepo::find_by_uid(pool.clone(), &address.uid).await?;

                match wallet {
                    Some(wallet) => {
                        if wallet.is_init == 1 {
                            ApiAccountRepo::init(
                                pool.clone(),
                                &address.address,
                                &address.chain_code,
                            )
                            .await?;

                            continue;
                        } else {
                            tracing::warn!(
                                "INIT_EXECUTOR: wallet not initialized: uid={}",
                                address.uid
                            );
                            return Err(crate::error::business::BusinessError::ApiWallet(
                                crate::error::business::api_wallet::ApiWalletError::WalletNotInit,
                            )
                            .into());
                        }
                    }
                    None => {
                        tracing::warn!("INIT_EXECUTOR: wallet not found: uid={}", address.uid);
                        return Err(crate::error::business::BusinessError::ApiWallet(
                            crate::error::business::api_wallet::ApiWalletError::WalletNotInit,
                        )
                        .into());
                    }
                }
            }

            tracing::info!(
                batch_id = ?init_req.batch_id,
                address_count = init_req.address_list.0.len(),
                "INIT_EXECUTOR: init task completed successfully"
            );
            Ok(())
        }
        Err(e) => {
            tracing::error!(
                batch_id = ?init_req.batch_id,
                address_count = init_req.address_list.0.len(),
                error = %e,
                "INIT_EXECUTOR: init task failed"
            );
            Err(ServiceError::from(e))
        }
    }
}
