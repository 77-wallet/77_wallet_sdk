use wallet_database::{entities::assets::AssetsIdVo, repositories::api_assets::ApiAssetsRepo};

pub(crate) struct ApiAssetsDomain;

impl ApiAssetsDomain {
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
}
