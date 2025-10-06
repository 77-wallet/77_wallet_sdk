use crate::{
    api::ReturnType,
    manager::WalletManager,
    response_vo::{account::Balance, api_wallet::assets::ApiAccountChainAssetList},
    service::api_wallet::asset::ApiAssetsService,
};

impl WalletManager {
    pub async fn get_api_assets_list(
        &self,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
    ) -> ReturnType<ApiAccountChainAssetList> {
        ApiAssetsService::new()
            .get_api_assets_list(wallet_address, account_id, chain_code, None)
            .await
    }

    // 根据钱包去同步资产
    pub async fn sync_api_assets_by_wallet(
        &self,
        wallet_address: String,
        account_id: Option<u32>,
        symbol: Vec<String>,
    ) -> ReturnType<()> {
        let res = ApiAssetsService::new()
            .sync_assets_by_wallet_backend(wallet_address, account_id, symbol)
            .await;

        if let Err(e) = res {
            tracing::error!("sync_assets error: {}", e);
            return Err(e);
        }

        Ok(())
    }

    // 查询链上的余额，并更新本地表
    pub async fn api_chain_balance(
        &self,
        address: String,
        chain_code: String,
        token_address: String,
    ) -> ReturnType<Balance> {
        ApiAssetsService::new().chain_balance(&address, &chain_code, &token_address).await
    }
}

#[cfg(test)]
mod test {
    use crate::test::env::get_manager;
    use anyhow::Result;

    #[tokio::test]
    async fn test_sync_api_assets_by_wallet() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let wallet_address = "0xF1C1FE41b1c50188faFDce5f21638e1701506f1b".to_string();
        let index = None;

        let res = wallet_manager.sync_api_assets_by_wallet(wallet_address, index, vec![]).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_api_assets_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        // let address = "0x531cCB9d552CBC5e16F0247b5657A5CDF2D77097";
        let address = "0xF1C1FE41b1c50188faFDce5f21638e1701506f1b";
        let chain_code = None;

        let account_id = Some(1);

        let _ = wallet_manager.set_currency("USD").await;
        let res = wallet_manager.get_api_assets_list(address, account_id, chain_code).await?;
        // tracing::info!("get_account_chain_assets: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res)?;
        tracing::info!("get_account_chain_assets: {}", res);
        Ok(())
    }
}
