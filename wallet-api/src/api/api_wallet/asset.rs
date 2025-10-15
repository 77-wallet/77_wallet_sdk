use crate::{
    api::ReturnType,
    manager::WalletManager,
    response_vo::{
        self, account::Balance, api_wallet::assets::ApiAccountChainAssetList,
        assets::GetAccountAssetsRes,
    },
    service::api_wallet::asset::ApiAssetsService,
};

impl WalletManager {
    pub async fn get_api_assets_list(
        &self,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
    ) -> ReturnType<ApiAccountChainAssetList> {
        ApiAssetsService::new(self.ctx)
            .get_api_assets_list(wallet_address, account_id, chain_code, None)
            .await
    }

    // api钱包添加资产
    pub async fn api_add_assets(&self, req: crate::request::coin::AddCoinReq) -> ReturnType<()> {
        ApiAssetsService::new(self.ctx).add_assets(req).await
    }

    pub async fn api_remove_assets(
        &self,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_list: response_vo::chain::ChainList,
    ) -> ReturnType<()> {
        ApiAssetsService::new(self.ctx).remove_assets(wallet_address, account_id, chain_list, None).await
    }

    // 已添加的币种列表
    pub async fn api_added_coin_list(
        &self,
        address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        keyword: Option<&str>,
        is_multisig: Option<bool>,
    ) -> ReturnType<crate::response_vo::coin::CoinInfoList> {
        ApiAssetsService::new(self.ctx).get_added_coin_list(address, account_id, chain_code, keyword, is_multisig)
            .await
    }

    // 根据钱包去同步资产
    pub async fn sync_api_assets_by_wallet(
        &self,
        wallet_address: String,
        account_id: Option<u32>,
        symbol: Vec<String>,
    ) -> ReturnType<()> {
        let res = ApiAssetsService::new(self.ctx)
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
        ApiAssetsService::new(self.ctx).chain_balance(&address, &chain_code, &token_address).await
    }

    // 资产列表
    pub async fn get_assets_list(
        &self,
        address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        is_multisig: Option<bool>,
    ) -> ReturnType<ApiAccountChainAssetList> {
        ApiAssetsService::new(self.ctx).get_account_chain_assets(address, account_id, chain_code, is_multisig)
            .await
    }

    // 账户的总资产
    pub async fn get_api_account_assets(
        &self,
        account_id: u32,
        wallet_address: &str,
        chain_code: Option<String>,
    ) -> ReturnType<GetAccountAssetsRes> {
        ApiAssetsService::new(self.ctx).get_account_assets(account_id, wallet_address, chain_code).await
    }

    pub async fn get_api_assets(
        &self,
        address: &str,
        account_id: Option<u32>,
        chain_code: &str,
        token_address: Option<String>,
    ) -> ReturnType<crate::response_vo::assets::CoinAssets> {
        let token_address = token_address.filter(|s| !s.is_empty());
        ApiAssetsService::new(self.ctx).detail(address, account_id, chain_code, token_address).await
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
        let address = "0x0d8B30ED6837b2EF0465Be9EE840700A589eaDB6";
        let chain_code = None;

        let account_id = Some(1);

        let _ = wallet_manager.set_currency("USD").await;
        let res = wallet_manager.get_api_assets_list(address, account_id, chain_code).await?;
        // tracing::info!("get_account_chain_assets: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res)?;
        tracing::info!("get_account_chain_assets: {}", res);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_assets_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        // let address = "0x531cCB9d552CBC5e16F0247b5657A5CDF2D77097";
        let address = "0x0d8B30ED6837b2EF0465Be9EE840700A589eaDB6";
        let chain_code = None;

        let account_id = Some(1);

        let _ = wallet_manager.set_currency("USD").await;
        let res = wallet_manager.get_assets_list(address, account_id, chain_code, None).await?;
        // tracing::info!("get_account_chain_assets: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res)?;
        tracing::info!("get_assets_list: {}", res);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_api_account_assets() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        // let address = "0x531cCB9d552CBC5e16F0247b5657A5CDF2D77097";
        let address = "0x0d8B30ED6837b2EF0465Be9EE840700A589eaDB6";
        let chain_code = None;

        let account_id = 1;

        let _ = wallet_manager.set_currency("USD").await;
        let res = wallet_manager.get_api_account_assets(account_id, address, chain_code).await?;
        // tracing::info!("get_account_chain_assets: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res)?;
        tracing::info!("get_assets_list: {}", res);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_api_assets() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        // let address = "0x531cCB9d552CBC5e16F0247b5657A5CDF2D77097";
        let address = "0x0d8B30ED6837b2EF0465Be9EE840700A589eaDB6";
        let chain_code = "tron";
        let token_address = Some("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string());

        let account_id = Some(1);

        let _ = wallet_manager.set_currency("USD").await;
        let res =
            wallet_manager.get_api_assets(address, account_id, chain_code, token_address).await?;
        // tracing::info!("get_account_chain_assets: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res)?;
        tracing::info!("get_assets_list: {}", res);
        Ok(())
    }
}
