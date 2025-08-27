use wallet_database::{
    entities::{api_assets::ApiAssetsEntity, assets::AssetsId},
    repositories::api_assets::ApiAssetsRepo,
};

pub(crate) struct ApiAssetsDomain;

impl ApiAssetsDomain {
    pub async fn assets(
        chain_code: &str,
        symbol: &str,
        from: &str,
        token_address: Option<String>,
    ) -> Result<ApiAssetsEntity, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let assets_id = AssetsId {
            address: from.to_string(),
            chain_code: chain_code.to_string(),
            symbol: symbol.to_string(),
            token_address,
        };
        let assets = ApiAssetsRepo::find_by_id(&pool, &assets_id)
            .await?
            .ok_or(crate::BusinessError::Assets(crate::AssetsError::NotFound))?;

        Ok(assets)
    }

    pub async fn update_balance(
        address: &str,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        balance: &str,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let assets_id = AssetsId {
            address: address.to_string(),
            chain_code: chain_code.to_string(),
            symbol: symbol.to_string(),
            token_address,
        };

        // 查询余额
        let asset = ApiAssetsRepo::find_by_id(&pool, &assets_id).await?;
        if let Some(asset) = asset {
            // 余额不一致
            if asset.balance != balance {
                // 更新本地余额后在上报后端
                ApiAssetsRepo::update_balance(&pool, &assets_id, balance)
                    .await
                    .map_err(crate::ServiceError::Database)?;

                // 上报后端修改余额
                let backend = crate::manager::Context::get_global_backend_api()?;
                let rs = backend.wallet_assets_refresh_bal(address, chain_code, symbol).await;
                if let Err(e) = rs {
                    tracing::warn!("upload balance refresh error = {}", e);
                }
            }
        }

        Ok(())
    }
}
