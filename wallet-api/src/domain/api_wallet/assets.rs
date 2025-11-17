use std::sync::Arc;

use futures::{StreamExt, stream};
use tokio::sync::Semaphore;
use wallet_database::{
    entities::{
        api_assets::ApiCreateAssetsVo,
        api_coin::ApiCoinEntity,
        assets::{AssetsId, AssetsIdVo},
    },
    repositories::api_wallet::{account::ApiAccountRepo, assets::ApiAssetsRepo},
};
use wallet_transport_backend::request::TokenQueryPriceReq;

use crate::{
    domain::{
        assets::{BalanceTask, BalanceTasks},
        chain::adapter::ChainAdapterFactory,
    },
    response_vo::account::BalanceInfo,
};

pub struct ApiAssetsDomain;

impl ApiAssetsDomain {
    pub(crate) async fn init_default_api_assets(
        coins: &[ApiCoinEntity],
        address: &str,
        chain_code: &str,
        req: &mut TokenQueryPriceReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        for coin in coins {
            if chain_code == coin.chain_code {
                let assets_id =
                    AssetsId::new(address, &coin.chain_code, &coin.symbol, coin.token_address());
                let assets =
                    ApiCreateAssetsVo::new(assets_id, coin.decimals, coin.protocol.clone(), 0)
                        .with_name(&coin.name)
                        .with_u256(alloy::primitives::U256::default(), coin.decimals)?;
                if coin.price.is_empty() {
                    req.insert(
                        chain_code,
                        &assets.assets_id.token_address.clone().unwrap_or_default(),
                    );
                }
                ApiAssetsRepo::upsert_assets(&pool, assets).await?;
            }
        }
        Ok(())
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
        Self::do_async_balance(addr, chain_code, symbol).await
    }

    async fn do_async_balance(
        addr: Vec<String>,
        chain_code: Option<String>,
        symbol: Vec<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut assets = ApiAssetsRepo::list(&pool, addr, chain_code).await?;
        if !symbol.is_empty() {
            assets.retain(|asset| symbol.contains(&asset.symbol));
        }

        tracing::info!("assets: {assets:#?}");
        let results = ApiChainBalance::sync_address_balance(assets.as_slice()).await?;

        // let mut done = 0;
        for (assets_id, balance) in &results {
            tracing::info!("assets_id: {assets_id:#?}, balance: {balance:#?}");
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
                    if let Some(account) =
                        ApiAccountRepo::find_one_by_address(&assets_id.address, &pool).await?
                    {
                        crate::infrastructure::asset_calc::on_asset_update(
                            &account.wallet_address,
                            &assets_id.address,
                            &assets_id.chain_code,
                            &assets_id.token_address.as_deref().unwrap_or_default(),
                        );
                    }

                    tracing::info!("更新余额成功: {:?}", assets_id);
                    // done += 1;
                }
                Err(e) => tracing::error!("更新余额出错: {:?}", e),
            }
        }

        // if done > 0 {
        //     if let Err(e) = FrontendNotifyEvent::new(NotifyEvent::ApiWalletSyncAssets).send().await
        //     {
        //         tracing::error!("send error: {}", e);
        //     }
        // }

        Ok(())
    }

    pub async fn get_api_wallet_assets(
        wallet_address: Option<&str>,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<BalanceInfo, crate::error::service::ServiceError> {
        let res = crate::infrastructure::asset_calc::get_balance_summary(
            wallet_address,
            account_id,
            chain_code,
        )
        .await?;

        Ok(res)
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

impl ApiChainBalance {
    pub(crate) async fn sync_address_balance(
        assets: impl Into<BalanceTasks>,
    ) -> Result<Vec<(AssetsId, String)>, crate::error::service::ServiceError> {
        // 限制最大并发数为 10
        let sem = Arc::new(Semaphore::new(10));
        let tasks: BalanceTasks = assets.into();

        // 并发获取余额并格式化
        let results = stream::iter(tasks.0)
            .map(|task| Self::fetch_balance(task, sem.clone()))
            .buffer_unordered(10)
            .filter_map(|x| async move { x })
            .collect::<Vec<_>>()
            .await;
        Ok(results)
    }

    // 从任务获取余额并返回结果
    async fn fetch_balance(task: BalanceTask, sem: Arc<Semaphore>) -> Option<(AssetsId, String)> {
        // 获取并发许可
        let _permit = sem.acquire().await.ok()?;
        // 获取适配器
        let adapter = ChainAdapterFactory::get_api_wallet_transaction_adapter(&task.chain_code)
            .await
            .map_err(|e| {
                tracing::error!("获取API链详情出错: {}，链代码: {}", e, task.chain_code.clone())
            })
            .ok()?;

        // 获取余额
        let raw = adapter
            .balance(&task.address, task.token_address.clone())
            .await
            .map_err(|e| {
                tracing::error!(
                    "获取API余额出错: 地址={}, 链={}, 符号={}, token={:?}, 错误={}",
                    task.address,
                    task.chain_code,
                    task.symbol,
                    task.token_address,
                    e
                )
            })
            .ok()?;

        // 格式化
        let bal_str = wallet_utils::unit::format_to_string(raw, task.decimals)
            .unwrap_or_else(|_| "0".to_string());

        // 构建 ID
        let id = AssetsId {
            address: task.address,
            chain_code: task.chain_code,
            symbol: task.symbol,
            token_address: task.token_address,
        };

        Some((id, bal_str))
    }
}
