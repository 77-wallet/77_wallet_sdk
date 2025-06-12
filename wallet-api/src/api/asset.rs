use crate::api::ReturnType;
use crate::response_vo::assets::{AccountChainAssetList, GetAccountAssetsRes};
use crate::service::asset::AssetsService;

impl crate::WalletManager {
    pub async fn add_coin(&self, req: crate::request::coin::AddCoinReq) -> ReturnType<()> {
        AssetsService::new(self.repo_factory.resource_repo())
            .add_coin(
                &req.wallet_address,
                Some(req.account_id),
                &req.symbol,
                req.chain_code,
                None,
            )
            .await?
            .into()
    }

    pub async fn add_regular_coin(&self, req: crate::request::coin::AddCoinReq) -> ReturnType<()> {
        AssetsService::new(self.repo_factory.resource_repo())
            .add_coin(
                &req.wallet_address,
                Some(req.account_id),
                &req.symbol,
                req.chain_code,
                Some(false),
            )
            .await?
            .into()
    }

    pub async fn add_multisig_coin(
        &self,
        req: crate::request::coin::AddMultisigCoinReq,
    ) -> ReturnType<()> {
        AssetsService::new(self.repo_factory.resource_repo())
            .add_coin(&req.address, None, &req.symbol, None, Some(true))
            .await?
            .into()
    }

    pub async fn get_assets(
        &self,
        address: &str,
        account_id: Option<u32>,
        chain_code: &str,
        symbol: &str,
    ) -> ReturnType<crate::response_vo::assets::CoinAssets> {
        AssetsService::new(self.repo_factory.resource_repo())
            .detail(address, account_id, chain_code, symbol)
            .await?
            .into()
    }

    pub async fn remove_coin(
        &self,
        wallet_address: &str,
        account_id: u32,
        symbol: &str,
    ) -> ReturnType<()> {
        AssetsService::new(self.repo_factory.resource_repo())
            .remove_coin(wallet_address, Some(account_id), symbol, None)
            .await?
            .into()
    }

    pub async fn remove_regular_coin(&self, address: &str, symbol: &str) -> ReturnType<()> {
        AssetsService::new(self.repo_factory.resource_repo())
            .remove_coin(address, None, symbol, Some(false))
            .await?
            .into()
    }

    pub async fn remove_multisig_coin(&self, address: &str, symbol: &str) -> ReturnType<()> {
        AssetsService::new(self.repo_factory.resource_repo())
            .remove_coin(address, None, symbol, Some(true))
            .await?
            .into()
    }

    /// 获取普通账户已添加的币列表
    pub async fn get_coin_list(
        &self,
        address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        keyword: Option<&str>,
        is_multisig: Option<bool>,
    ) -> ReturnType<crate::response_vo::coin::CoinInfoList> {
        AssetsService::new(self.repo_factory.resource_repo())
            .get_coin_list(address, account_id, chain_code, keyword, is_multisig)
            .await?
            .into()
    }

    pub async fn get_all_account_assets(
        &self,
        account_id: u32,
        wallet_address: Option<&str>,
    ) -> ReturnType<GetAccountAssetsRes> {
        AssetsService::new(self.repo_factory.resource_repo())
            .get_all_account_assets(account_id, wallet_address)
            .await?
            .into()
    }

    /// 获取普通账户总资产
    pub async fn get_account_assets(
        &self,
        account_id: u32,
        wallet_address: &str,
        chain_code: Option<String>,
    ) -> ReturnType<GetAccountAssetsRes> {
        AssetsService::new(self.repo_factory.resource_repo())
            .get_account_assets(account_id, wallet_address, chain_code)
            .await?
            .into()
    }

    /// 获取多签账户总资产
    pub async fn get_multisig_account_assets(
        &self,
        address: &str,
    ) -> ReturnType<GetAccountAssetsRes> {
        AssetsService::new(self.repo_factory.resource_repo())
            .get_multisig_account_assets(address)
            .await?
            .into()
    }

    pub async fn get_assets_list_v2(
        &self,
        address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        is_multisig: Option<bool>,
    ) -> ReturnType<AccountChainAssetList> {
        AssetsService::new(self.repo_factory.resource_repo())
            .get_account_chain_assets_v2(address, account_id, chain_code, is_multisig)
            .await?
            .into()
    }

    // 根据资产地址、后端同步。
    pub async fn sync_assets(
        &self,
        addr: String,
        chain_code: Option<String>,
        symbol: Vec<String>,
    ) -> ReturnType<()> {
        // let res = AssetsService::new(self.repo_factory.resource_repo())
        //     .sync_assets_from_backend(addr, chain_code, symbol)
        //     .await;
        let res = AssetsService::new(self.repo_factory.resource_repo())
            .sync_assets_by_addr(addr, chain_code, symbol)
            .await;
        if let Err(e) = res {
            tracing::error!("sync_assets error: {}", e);
        }
        ().into()
    }

    // 根据资产地址、链以及符号来同步余额(直接重链上同步余额)。
    pub async fn sync_balance_from_chain(
        &self,
        addr: String,
        chain_code: Option<String>,
        symbol: Vec<String>,
    ) -> ReturnType<()> {
        let res = AssetsService::new(self.repo_factory.resource_repo())
            .sync_assets_by_addr(vec![addr], chain_code, symbol)
            .await;
        if let Err(e) = res {
            tracing::error!("sync_assets error: {}", e);
        }

        ().into()
    }

    // 根据钱包去同步资产
    pub async fn sync_assets_by_wallet(
        &self,
        wallet_address: String,
        account_id: Option<u32>,
        symbol: Vec<String>,
    ) -> ReturnType<()> {
        let res = AssetsService::new(self.repo_factory.resource_repo())
            .sync_assets_by_wallet(wallet_address, account_id, symbol)
            .await;
        if let Err(e) = res {
            tracing::error!("sync_assets error: {}", e);
        }

        ().into()
    }
}

#[cfg(test)]
mod test {
    use crate::test::env::get_manager;
    use anyhow::Result;

    #[tokio::test]
    async fn test_add_assets() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let add_coin_req = crate::request::coin::AddCoinReq {
            account_id: 1,
            symbol: "BNB".to_string(),
            wallet_address: "0x7d2485c67AD614CE2CE8E6759c24e6e73e3de26f".to_string(),
            chain_code: None,
        };
        let res = wallet_manager.add_coin(add_coin_req).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_add_multisig_assets() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let add_coin_req = crate::request::coin::AddMultisigCoinReq {
            symbol: "USDT".to_string(),
            address: "0x3bAc24b73c7A03C8715697cA1646a6f85B91023a".to_string(),
        };
        let res = wallet_manager.add_multisig_coin(add_coin_req).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_assets() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let address = "0xC24FCE9Ae9dEF3d18B926B363EaE25a22Ed71F9f";
        let account_id = None;
        let chain_code = "bnb";
        let symbol = "USDT";
        let res = wallet_manager
            .get_assets(address, account_id, chain_code, symbol)
            .await;
        tracing::info!("res: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_remove_coin() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        // let wallet_address = "0xd8dc4B7daEfc0C993d1A7d3E2D4Dc998436032b3";
        // let wallet_address = "0xa32D8B667Fd6d2e30C1E6D7fE6E4319Bf1D4D310";
        let wallet_address = "0xE63EB4fba134978EfdD529BBea8a2F64B30068C1";
        // let symbol = "LTC";
        // let symbol = "BEANS";
        let symbol = "USDT";
        let res = wallet_manager.remove_coin(wallet_address, 1, symbol).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_remove_multisig_coin() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let res = wallet_manager
            .remove_multisig_coin("0x3bAc24b73c7A03C8715697cA1646a6f85B91023a", "USDT")
            .await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_coin_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        // let keyword = Some("t");
        let keyword = None;
        // let chain_code = Some("Tron");
        let chain_code = None;
        // let is_multisig = None;
        let is_multisig = Some(false);
        let wallet_address = "0x82C818D352BAf6cC7dd007B89E5CC82B4DAF2c9c";
        let res = wallet_manager
            .get_coin_list(wallet_address, Some(1), chain_code, keyword, is_multisig)
            .await;
        tracing::info!("res: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_multisig_coin_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        // let keyword = Some("t");
        let keyword = None;
        let chain_code = Some("tron".to_string());
        // let chain_code = None;
        // let is_multisig = None;
        let is_multisig = Some(true);

        let address = "TRbHD77Y6WWDaz9X5esrVKwEVwRM4gTw6N";
        let res = wallet_manager
            .get_coin_list(address, None, chain_code, keyword, is_multisig)
            .await;
        tracing::info!("res: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_open_account_pk() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, test_params) = get_manager().await?;
        // let address = "TCWBCCuapMcnrSxhudiNshq1UK4nCvZren";
        // let address = "A9gBqKMQDWUYNiHHpHakSEsztKuxxN838EWGuG2WKc6F";
        let address = "0x0FDDDc2C86547328a0125468380d592d63625ba2";
        // let address = "0xA8eEE0468F2D87D7603ec72c988c5f24C11fEd32";
        let account_id = 1;
        let res = wallet_manager
            .get_account_private_key(
                &test_params.create_wallet_req.wallet_password,
                address,
                account_id,
            )
            .await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_all_account_assets() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        // let address = "TCWBCCuapMcnrSxhudiNshq1UK4nCvZren";
        let address = "0xa32D8B667Fd6d2e30C1E6D7fE6E4319Bf1D4D310";
        // let address = "0xA8eEE0468F2D87D7603ec72c988c5f24C11fEd32";
        let account_asset = wallet_manager
            .get_all_account_assets(1, Some(address))
            .await;
        tracing::info!("account_asset: {account_asset:?}");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_account_assets() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.set_currency("USD").await;
        // let address = "TCWBCCuapMcnrSxhudiNshq1UK4nCvZren";
        // let address = "0x9e2BEf062f301C85589E342d569058Fd4a1334d7";
        let address = "0xDA32fc1346Fa1DF9719f701cbdd6855c901027C1";
        // let address = "0xA8eEE0468F2D87D7603ec72c988c5f24C11fEd32";
        let chain_code = Some("sol".to_string());
        let account_asset = wallet_manager
            .get_account_assets(1, address, chain_code)
            .await;
        tracing::info!("account_asset: {account_asset:?}");
        let res = wallet_utils::serde_func::serde_to_string(&account_asset).unwrap();
        tracing::info!("res: {res}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_multisig_account_assets() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let address = "TT4QgNx2rVD35tYU1LJ6tH5Ya1bxmannBK";
        let account_asset = wallet_manager.get_multisig_account_assets(address).await;
        tracing::info!("account_asset: {account_asset:?}");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_assets_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        // let address = "0x531cCB9d552CBC5e16F0247b5657A5CDF2D77097";
        let address = "0x0E1E806fdB77Eb4a67F3c3CCCBA58d62F4325077";
        let chain_code = None;
        let is_multisig = None;
        let account_id = Some(1);

        wallet_manager.set_currency("USD").await;
        let res = wallet_manager
            .get_assets_list_v2(address, account_id, chain_code, is_multisig)
            .await;
        tracing::info!("get_account_chain_assets: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res)?;
        tracing::info!("get_account_chain_assets: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_multisig_assets_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let address = "TBk86hq1e8C1gNX6RDXhk1wLamwzKnotmo";
        // let chain_code = None;
        let chain_code = Some("tron".to_string());
        let is_multisig = Some(true);
        let account_id = None;

        let res = wallet_manager
            .get_assets_list_v2(address, account_id, chain_code, is_multisig)
            .await;
        tracing::info!("res: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res)?;
        tracing::info!("res: {res:?}");
        Ok(())
    }
}
