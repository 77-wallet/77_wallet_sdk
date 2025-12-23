use std::sync::Arc;

use futures::{StreamExt, stream};
use tokio::sync::Semaphore;
use wallet_database::{
    entities::{
        api_assets::ApiCreateAssetsVo,
        api_coin::ApiCoinEntity,
        assets::{AssetsId, AssetsIdVo},
    },
    repositories::{
        api_wallet::{account::ApiAccountRepo, assets::ApiAssetsRepo},
        exchange_rate::ExchangeRateRepo,
    },
};
use wallet_transport_backend::request::TokenQueryPriceReq;

use crate::{
    domain::{
        app::config::ConfigDomain,
        assets::{BalanceTask, BalanceTasks},
        chain::adapter::ChainAdapterFactory,
    },
    infrastructure::asset_calc::actor_model::AssetKey,
    response_vo::standard_wallet::account::BalanceInfo,
};

pub struct ApiAssetsDomain;

impl ApiAssetsDomain {
    pub(crate) async fn init_default_api_assets(
        wallet_address: &str,
        coins: &[ApiCoinEntity],
        address: &str,
        chain_code: &str,
        req: &mut TokenQueryPriceReq,
    ) -> Result<Vec<AssetKey>, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let mut asset_keys = Vec::new();
        for coin in coins {
            if chain_code == coin.chain_code {
                let assets_id =
                    AssetsId::new(address, &coin.chain_code, &coin.symbol, coin.token_address());
                let assets =
                    ApiCreateAssetsVo::new(assets_id, coin.decimals, coin.protocol.clone(), 0)
                        .with_name(&coin.name)
                        .with_u256(alloy::primitives::U256::default(), coin.decimals)?;
                let token_address = assets.assets_id.token_address.clone().unwrap_or_default();
                if coin.price.is_empty() {
                    req.insert(chain_code, token_address.as_str());
                }
                ApiAssetsRepo::upsert_assets(&pool, assets).await?;

                asset_keys.push(AssetKey::new(wallet_address, address, chain_code, &token_address));
            }
        }

        Ok(asset_keys)
    }

    pub async fn update_balance(
        address: &str,
        chain_code: &str,
        token_address: Option<String>,
        balance: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let assets_id = AssetsIdVo { address, chain_code, token_address: token_address.clone() };

        // 查询余额
        let asset = ApiAssetsRepo::find_by_id(&pool, &assets_id).await?;
        if let Some(asset) = asset {
            // 余额不一致
            if asset.balance != balance {
                // 更新本地余额后在上报后端
                ApiAssetsRepo::update_balance(
                    &pool,
                    &asset.address,
                    chain_code,
                    token_address,
                    balance,
                )
                .await?;

                // 上报后端修改余额
                let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
                let rs =
                    backend.wallet_assets_refresh_bal(address, chain_code, &asset.symbol).await;
                if let Err(e) = rs {
                    tracing::warn!("upload balance refresh error = {}", e);
                }
            }
        }

        Ok(())
    }

    // 根据钱包地址来同步资产余额( 目前不需要在进行使用 )
    // pub async fn sync_assets_by_wallet(
    //     wallet_address: &str,
    //     account_id: Option<u32>,
    //     symbol: Vec<String>,
    // ) -> Result<(), crate::error::service::ServiceError> {
    //     let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

    //     let list =
    //         ApiAccountRepo::list_by_wallet_address(&pool, wallet_address, account_id, None).await?;

    //     // 获取地址
    //     let addr = list.iter().map(|a| a.address.clone()).collect::<Vec<String>>();

    //     Self::do_async_balance(pool, addr, None, symbol).await
    // }

    // async fn do_async_balance(
    //     pool: DbPool,
    //     addr: Vec<String>,
    //     chain_code: Option<String>,
    //     symbol: Vec<String>,
    // ) -> Result<(), crate::error::service::ServiceError> {
    //     let mut assets = ApiAssetsRepo::list(
    //         &pool, // , addr, chain_code, None, None
    //     )
    //     .await?;
    //     if !symbol.is_empty() {
    //         assets.retain(|asset| symbol.contains(&asset.symbol));
    //     }

    //     let results = ChainBalance::sync_address_balance(assets.as_slice()).await?;

    //     for (assets_id, balance) in &results {
    //         if let Err(e) = ApiAssetsRepo::update_balance(
    //             &pool,
    //             &assets_id.address,
    //             &assets_id.chain_code,
    //             assets_id.token_address.clone(),
    //             balance,
    //         )
    //         .await
    //         {
    //             tracing::error!("更新余额出错: {}", e);
    //         }
    //     }

    //     Ok(())
    // }

    pub async fn sync_assets_by_addr_chain(
        addr: Vec<String>,
        chain_code: Option<String>,
        symbol: Vec<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        Self::do_async_balance(addr, chain_code, symbol, 0).await
    }

    pub async fn sync_assets_by_addr_chain_with_retry(
        addr: Vec<String>,
        chain_code: Option<String>,
        symbol: Vec<String>,
        retry_count: u32,
    ) -> Result<(), crate::error::service::ServiceError> {
        Self::do_async_balance(addr, chain_code, symbol, retry_count).await
    }

    async fn do_async_balance(
        addr: Vec<String>,
        chain_code: Option<String>,
        symbol: Vec<String>,
        retry_count: u32,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        tracing::info!(
            "开始异步余额同步: addr_count={}, chain_code={:?}, symbols={:?}, retry_count={}",
            addr.len(),
            chain_code,
            symbol,
            retry_count
        );

        let mut assets = ApiAssetsRepo::list(&pool, addr.clone(), chain_code.clone()).await?;
        let original_count = assets.len();

        if !symbol.is_empty() {
            let mut filtered_assets = Vec::new();
            let mut filtered_out = Vec::new();

            // 优化：使用大小写不敏感的匹配，同时记录被过滤的资产
            let symbol_set: std::collections::HashSet<String> =
                symbol.iter().map(|s| s.to_uppercase()).collect();

            for asset in assets {
                if symbol_set.contains(&asset.symbol.to_uppercase()) {
                    filtered_assets.push(asset);
                } else {
                    filtered_out.push(format!("{}/{}", asset.symbol, asset.address));
                }
            }

            if !filtered_out.is_empty() {
                tracing::debug!(
                    "过滤掉 {} 个资产（symbol 不匹配）: {:?}",
                    filtered_out.len(),
                    filtered_out
                );
            }

            assets = filtered_assets;
        }

        tracing::info!(
            "查询到 {} 个资产（过滤前: {}），需要同步: {:?}",
            assets.len(),
            original_count,
            assets.iter().map(|a| format!("{}/{}", a.symbol, a.address)).collect::<Vec<_>>()
        );

        if assets.is_empty() {
            tracing::warn!(
                "没有找到需要同步的资产: addr={:?}, chain_code={:?}, symbols={:?}",
                addr,
                chain_code,
                symbol
            );
            return Ok(());
        }

        let sync_result = ApiChainBalance::sync_address_balance(assets.as_slice()).await?;

        tracing::info!(
            "余额查询完成: 成功={}, 失败={}, 总数={}",
            sync_result.success.len(),
            sync_result.failed_tasks.len(),
            assets.len()
        );

        let mut success_count = 0;
        let mut fail_count = 0;
        let mut retry_tasks = Vec::new();

        // 先计算总数，避免移动后无法访问
        let total_count = sync_result.success.len() + sync_result.failed_tasks.len();

        // 处理失败的任务，区分可重试和不可重试的错误
        for (failed_task, err) in sync_result.failed_tasks {
            let is_retryable = err.is_network_error();

            if is_retryable {
                tracing::warn!(
                    "余额查询失败（可重试）: address={}, chain_code={}, symbol={}, error={}",
                    failed_task.address,
                    failed_task.chain_code,
                    failed_task.symbol,
                    err
                );
                retry_tasks.push(failed_task);
            } else {
                tracing::error!(
                    "余额查询失败（不可重试）: address={}, chain_code={}, symbol={}, error={}",
                    failed_task.address,
                    failed_task.chain_code,
                    failed_task.symbol,
                    err
                );
                fail_count += 1;
            }
        }

        // 优化：批量查询账户，解决 N+1 查询问题
        let addresses: Vec<String> = sync_result
            .success
            .iter()
            .map(|(assets_id, _)| assets_id.address.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let accounts_map: std::collections::HashMap<
            String,
            wallet_database::entities::api_account::ApiAccountEntity,
        > = if !addresses.is_empty() {
            ApiAccountRepo::find_by_addresses(&addresses, &pool)
                .await?
                .into_iter()
                .map(|acc| (acc.address.clone(), acc))
                .collect()
        } else {
            std::collections::HashMap::new()
        };

        // 优化：批量更新余额，减少数据库往返次数
        if !sync_result.success.is_empty() {
            let updates: Vec<(String, String, Option<String>, String)> = sync_result
                .success
                .iter()
                .map(|(assets_id, balance)| {
                    (
                        assets_id.address.clone(),
                        assets_id.chain_code.clone(),
                        assets_id.token_address.clone(),
                        balance.clone(),
                    )
                })
                .collect();

            let asset_calc_actor_manager = crate::context::CONTEXT
                .get()
                .unwrap()
                .get_global_asset_calc_actor_manager()
                .await?;
            match ApiAssetsRepo::batch_update_balance(&pool, updates).await {
                Ok(_) => {
                    success_count = sync_result.success.len();

                    // 批量触发资产更新通知
                    let mut asset_keys = Vec::new();
                    for (assets_id, _) in &sync_result.success {
                        if let Some(account) = accounts_map.get(&assets_id.address) {
                            asset_keys.push(AssetKey::new(
                                &account.wallet_address,
                                &assets_id.address,
                                &assets_id.chain_code,
                                &assets_id.token_address.as_deref().unwrap_or_default(),
                            ));
                        }
                    }
                    if !asset_keys.is_empty() {
                        asset_calc_actor_manager.update_assets(&asset_keys).await?;
                    }

                    tracing::info!(
                        "批量更新余额成功: 成功数量={}, 涉及地址数={}",
                        success_count,
                        addresses.len()
                    );
                }
                Err(e) => {
                    fail_count = sync_result.success.len();
                    tracing::error!("批量更新余额失败: 失败数量={}, error={:?}", fail_count, e);

                    // 批量更新失败，回退到逐个更新（用于错误恢复）
                    tracing::warn!("回退到逐个更新模式");
                    let mut asset_keys = Vec::new();
                    for (assets_id, balance) in &sync_result.success {
                        match ApiAssetsRepo::update_balance(
                            &pool,
                            &assets_id.address,
                            &assets_id.chain_code,
                            assets_id.token_address.clone(),
                            balance,
                        )
                        .await
                        {
                            Ok(_) => {
                                success_count += 1;
                                fail_count -= 1;

                                if let Some(account) = accounts_map.get(&assets_id.address) {
                                    asset_keys.push(AssetKey::new(
                                        &account.wallet_address,
                                        &assets_id.address,
                                        &assets_id.chain_code,
                                        &assets_id.token_address.as_deref().unwrap_or_default(),
                                    ));
                                }

                                tracing::debug!(
                                    "单个更新余额成功: address={}, chain_code={}, symbol={}",
                                    assets_id.address,
                                    assets_id.chain_code,
                                    assets_id.symbol
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "单个更新余额失败: address={}, chain_code={}, symbol={}, error={:?}",
                                    assets_id.address,
                                    assets_id.chain_code,
                                    assets_id.symbol,
                                    e
                                );
                            }
                        }
                    }
                    if !asset_keys.is_empty() {
                        asset_calc_actor_manager.update_assets(&asset_keys).await?;
                    }
                }
            }
        }

        tracing::info!(
            "余额同步完成: 成功={}, 失败={}, 需要重试={}, 总数={}",
            success_count,
            fail_count,
            retry_tasks.len(),
            total_count
        );

        // 将可重试的任务进行延迟重试
        if !retry_tasks.is_empty() {
            let next_retry_count = retry_count + 1;
            Self::retry_failed_balance_tasks(retry_tasks, chain_code, next_retry_count).await?;
        }

        Ok(())
    }

    // 将失败的任务进行延迟重试
    // retry_count: 当前重试次数（首次失败时为1，第二次失败时为2，以此类推）
    async fn retry_failed_balance_tasks(
        failed_tasks: Vec<BalanceTask>,
        _chain_code: Option<String>,
        retry_count: u32,
    ) -> Result<(), crate::error::service::ServiceError> {
        if failed_tasks.is_empty() {
            return Ok(());
        }

        const MAX_RETRY_COUNT: u32 = 3;
        const INITIAL_RETRY_DELAY_SECS: u64 = 5;

        // 检查重试次数限制
        if retry_count >= MAX_RETRY_COUNT {
            tracing::error!(
                "资产同步重试次数已达到最大限制，放弃重试: retry_count={}, failed_task_count={}",
                retry_count,
                failed_tasks.len()
            );

            // 记录最终失败的任务详情
            for task in &failed_tasks {
                tracing::error!(
                    "最终失败的资产同步任务: address={}, chain_code={}, symbol={}, token_address={:?}",
                    task.address,
                    task.chain_code,
                    task.symbol,
                    task.token_address
                );
            }

            return Ok(());
        }

        // 计算指数退避延迟：2^(retry_count-1) * INITIAL_RETRY_DELAY_SECS
        // retry_count=1: 5秒, retry_count=2: 10秒, retry_count=3: 20秒
        let delay_secs = INITIAL_RETRY_DELAY_SECS * (1 << (retry_count.saturating_sub(1)));

        tracing::info!(
            "准备重试失败的资产同步任务: retry_count={}/{}, failed_task_count={}, 将在 {} 秒后重试",
            retry_count,
            MAX_RETRY_COUNT,
            failed_tasks.len(),
            delay_secs
        );

        // 按 chain_code + symbol + token_address 分组
        let mut grouped: std::collections::HashMap<(String, String, Option<String>), Vec<String>> =
            std::collections::HashMap::new();

        for task in failed_tasks {
            grouped
                .entry((task.chain_code.clone(), task.symbol.clone(), task.token_address.clone()))
                .or_default()
                .push(task.address);
        }

        // 延迟重试，避免立即重试导致的资源浪费和网络拥塞
        // 在 spawn 之前获取 inner_event_handle，确保 Send trait
        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        let inner_event_handle = if let Some(handles) = handles.upgrade() {
            Some(handles.get_global_inner_event_handle())
        } else {
            tracing::error!("Handles 已释放，无法重试失败的任务");
            return Ok(());
        };

        // 使用 tokio::spawn 异步执行延迟重试，避免阻塞当前流程
        // 将 HashMap 转换为 Vec，确保 Send trait
        let grouped_vec: Vec<_> = grouped.into_iter().collect();

        if let Some(inner_event_handle) = inner_event_handle {
            tokio::spawn(async move {
                tracing::info!(
                    "将在 {} 秒后重试失败的资产同步任务: retry_count={}/{}, 分组数={}",
                    delay_secs,
                    retry_count,
                    MAX_RETRY_COUNT,
                    grouped_vec.len()
                );

                tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;

                for ((chain_code, symbol, token_address), addr_list) in grouped_vec {
                    tracing::info!(
                        "开始重试资产同步任务 (重试 {}/{}): chain_code={}, symbol={}, token_address={:?}, addr_count={}",
                        retry_count,
                        MAX_RETRY_COUNT,
                        chain_code,
                        symbol,
                        token_address,
                        addr_list.len()
                    );

                    // 通过 InnerEvent 重试，这样可以确保重试任务也经过统一的事件处理流程
                    let data = crate::infrastructure::inner_event::SyncAssetsData::new(
                        addr_list,
                        chain_code,
                        vec![symbol],
                        token_address,
                    )
                    .with_retry_count(retry_count);

                    if let Err(e) = inner_event_handle.send(
                        crate::infrastructure::inner_event::InnerEvent::ApiWalletSyncAssets(data),
                    ) {
                        tracing::error!(
                            "重试时发送资产同步事件失败: retry_count={}, error={}",
                            retry_count,
                            e
                        );
                    }
                }
            });
        }

        Ok(())
    }

    pub async fn get_api_wallet_assets(
        wallet_address: Option<&str>,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<BalanceInfo, crate::error::service::ServiceError> {
        let asset_calc_actor_manager =
            crate::context::CONTEXT.get().unwrap().get_global_asset_calc_actor_manager().await?;
        let res = asset_calc_actor_manager
            .get_balance_summary(wallet_address, account_id, chain_code)
            .await?;

        Ok(res)
    }

    pub async fn get_api_wallet_assets_v2(
        wallet_address: Option<&str>,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<BalanceInfo, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let total = ApiAssetsRepo::get_api_wallet_total_assets_v2(
            &pool,
            wallet_address,
            account_id,
            chain_code,
        )
        .await?;

        let currency = ConfigDomain::get_currency().await?;
        let exchange_rate =
            ExchangeRateRepo::get_by_target_currency_or_default(&pool, &currency).await?;
        let cal_exchange_rate = |value: f64| {
            if exchange_rate.target_currency.to_uppercase() == "USD" {
                value
            } else {
                value * exchange_rate.rate
            }
        };

        Ok(BalanceInfo {
            amount: total.total_coins_quantity,
            currency,
            unit_price: None,
            fiat_value: Some(cal_exchange_rate(total.total_amount)),
        })
    }

    // pub async fn get_api_wallet_assets(
    //     wallet_address: &str,
    // ) -> Result<BalanceInfo, crate::error::service::ServiceError> {
    //     let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
    //     let api_wallet = ApiWalletRepo::find_by_address(&pool, wallet_address).await?.ok_or(
    //         crate::error::business::BusinessError::ApiWallet(
    //             crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
    //         ),
    //     )?;
    //     let balance_list = crate::infrastructure::asset_calc::get_wallet_balance_list().await?;
    //     // tracing::info!("get_api_wallet_assets balance_list: {balance_list:#?}");
    //     let res = if let Some(balance) = balance_list.get(&api_wallet.address) {
    //         balance.to_owned()
    //     } else {
    //         BalanceInfo::new_without_amount().await?
    //     };
    //     Ok(res)
    // }
}

pub(crate) struct ApiChainBalance;

#[derive(Debug)]
pub(crate) struct BalanceSyncResult {
    pub(crate) success: Vec<(AssetsId, String)>,
    pub(crate) failed_tasks: Vec<(BalanceTask, crate::error::service::ServiceError)>,
}

impl ApiChainBalance {
    pub(crate) async fn sync_address_balance(
        assets: impl Into<BalanceTasks>,
    ) -> Result<BalanceSyncResult, crate::error::service::ServiceError> {
        // 限制最大并发数为 10
        let sem = Arc::new(Semaphore::new(10));
        let tasks: BalanceTasks = assets.into();

        // 并发获取余额并格式化
        let results: Vec<_> = stream::iter(tasks.0)
            .map(|task| Self::fetch_balance(task, sem.clone()))
            .buffer_unordered(10)
            .collect::<Vec<_>>()
            .await;

        let mut success = Vec::new();
        let mut failed_tasks = Vec::new();

        for result in results {
            match result {
                Ok((id, balance)) => success.push((id, balance)),
                Err((task, err)) => failed_tasks.push((task, err)),
            }
        }

        Ok(BalanceSyncResult { success, failed_tasks })
    }

    // 从任务获取余额并返回结果，失败时返回错误信息
    async fn fetch_balance(
        task: BalanceTask,
        sem: Arc<Semaphore>,
    ) -> Result<(AssetsId, String), (BalanceTask, crate::error::service::ServiceError)> {
        // 先克隆所有需要的字段，避免所有权问题
        let address = task.address.clone();
        let chain_code = task.chain_code.clone();
        let symbol = task.symbol.clone();
        let token_address = task.token_address.clone();
        let decimals = task.decimals;

        // 获取并发许可
        let _permit = sem.acquire().await.map_err(|e| {
            let error_msg = format!("Semaphore acquire failed: {}", e);
            // 使用克隆的字段重新构造 BalanceTask
            (
                BalanceTask {
                    address: address.clone(),
                    chain_code: chain_code.clone(),
                    symbol: symbol.clone(),
                    decimals,
                    token_address: token_address.clone(),
                },
                crate::error::service::ServiceError::System(
                    crate::error::system::SystemError::Service(error_msg),
                ),
            )
        })?;

        // 获取适配器
        let adapter = ChainAdapterFactory::get_api_wallet_transaction_adapter(&chain_code)
            .await
            .map_err(|e| {
                let err = crate::error::service::ServiceError::from(e);
                let chain_code_clone = chain_code.clone();
                tracing::error!("获取API链详情出错: {}，链代码: {}", err, chain_code_clone);
                (
                    BalanceTask {
                        address: address.clone(),
                        chain_code: chain_code_clone,
                        symbol: symbol.clone(),
                        decimals,
                        token_address: token_address.clone(),
                    },
                    err,
                )
            })?;

        // 获取余额
        let raw = adapter.balance(&address, token_address.clone()).await.map_err(|e| {
            let err = crate::error::service::ServiceError::from(e);
            // 在错误处理中克隆所有需要的值
            let address_clone = address.clone();
            let chain_code_clone = chain_code.clone();
            let symbol_clone = symbol.clone();
            let token_address_clone = token_address.clone();
            tracing::error!(
                "获取API余额出错: 地址={}, 链={}, 符号={}, token={:?}, 错误={}",
                address_clone,
                chain_code_clone,
                symbol_clone,
                token_address_clone,
                err
            );
            // 重新构造 BalanceTask 以便返回
            (
                BalanceTask {
                    address: address_clone,
                    chain_code: chain_code_clone,
                    symbol: symbol_clone,
                    decimals,
                    token_address: token_address_clone,
                },
                err,
            )
        })?;

        tracing::debug!("获取API余额原始值: {:?}, 小数位数: {}", raw, decimals);
        // 格式化
        let bal_str =
            wallet_utils::unit::format_to_string(raw, decimals).unwrap_or_else(|_| "0".to_string());
        tracing::debug!(
            "获取API余额成功: 地址={}, 链={}, 符号={}, token={:?}, 余额={}",
            address,
            chain_code,
            symbol,
            token_address,
            bal_str
        );
        // 构建 ID
        let id = AssetsId { address, chain_code, symbol, token_address };

        Ok((id, bal_str))
    }
}
