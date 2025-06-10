use wallet_database::entities::chain::{ChainEntity, ChainWithNode};

use crate::{api::ReturnType, response_vo::chain::ChainAssets, service::chain::ChainService};

impl crate::WalletManager {
    pub async fn add_chain(&self, name: &str, chain_code: &str) -> ReturnType<()> {
        ChainService::new(self.repo_factory.resource_repo())
            .add(name, chain_code, &[], "")
            .await?
            .into()
    }

    pub async fn set_chain_node(&self, chain_code: &str, node_id: &str) -> ReturnType<()> {
        ChainService::new(self.repo_factory.resource_repo())
            .set_chain_node(chain_code, node_id)
            .await?
            .into()
    }

    pub async fn get_market_chain_list(&self) -> ReturnType<Vec<String>> {
        ChainService::new(self.repo_factory.resource_repo())
            .get_market_chain_list()
            .await?
            .into()
    }

    pub async fn sync_chains(&self) -> ReturnType<bool> {
        ChainService::new(self.repo_factory.resource_repo())
            .sync_chains()
            .await?
            .into()
    }

    pub async fn sync_wallet_chain_data(&self, wallet_password: &str) -> ReturnType<()> {
        ChainService::new(self.repo_factory.resource_repo())
            .sync_wallet_chain_data(wallet_password)
            .await?
            .into()
    }

    pub async fn get_hot_chain_list(&self) -> ReturnType<Vec<ChainEntity>> {
        ChainService::new(self.repo_factory.resource_repo())
            .get_hot_chain_list()
            .await?
            .into()
    }

    pub async fn get_setting_chain_list(&self) -> ReturnType<Vec<ChainWithNode>> {
        ChainService::new(self.repo_factory.resource_repo())
            .get_chain_list_with_node_info()
            .await?
            .into()
    }

    pub async fn get_protocol_list(&self, chain_code: &str) -> ReturnType<Option<ChainEntity>> {
        ChainService::new(self.repo_factory.resource_repo())
            .get_protocol_list(chain_code)
            .await?
            .into()
    }

    pub async fn get_chain_list(
        &self,
        wallet_address: &str,
        account_id: u32,
        symbol: &str,
    ) -> ReturnType<Vec<ChainAssets>> {
        ChainService::new(self.repo_factory.resource_repo())
            .get_chain_assets_list(wallet_address, Some(account_id), symbol, None)
            .await?
            .into()
    }

    pub async fn get_multisig_chain_list(
        &self,
        address: &str,
        symbol: &str,
    ) -> ReturnType<Vec<ChainAssets>> {
        ChainService::new(self.repo_factory.resource_repo())
            .get_chain_assets_list(address, None, symbol, Some(true))
            .await?
            .into()
    }
}

#[cfg(test)]
mod tests {
    use crate::test::env::get_manager;
    use anyhow::Result;

    #[tokio::test]
    async fn test_get_chain_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        // let address = "0x0996dc2A80F35D7075C426bf0Ac6e389e0AB99Fc";
        let address = "0x57CF28DD99cc444A9EEEEe86214892ec9F295480";
        // let symbol = "LTC";
        let symbol = "USDT";
        let res = wallet_manager.get_chain_list(address, 1, symbol).await;
        let res = wallet_utils::serde_func::serde_to_string(&res)?;
        tracing::info!("res: {res}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_market_chain_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let res = wallet_manager.get_market_chain_list().await;
        let res = wallet_utils::serde_func::serde_to_string(&res)?;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_multisig_chain_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        // let address = "0x0996dc2A80F35D7075C426bf0Ac6e389e0AB99Fc";
        let address = "TBk86hq1e8C1gNX6RDXhk1wLamwzKnotmo";
        let symbol = "TRX";
        let res = wallet_manager
            .get_multisig_chain_list(address, symbol)
            .await;
        let res = wallet_utils::serde_func::serde_to_string(&res)?;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_setting_chain_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let get_config_chain_list = wallet_manager.get_setting_chain_list().await;
        let get_config_chain_list =
            wallet_utils::serde_func::serde_to_string(&get_config_chain_list).unwrap();

        tracing::info!("get_config_chain_list: {get_config_chain_list:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_set_chain_node() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let chain_code = "tron";
        let node_id = "test";
        let set_chain_node = wallet_manager.set_chain_node(chain_code, node_id).await;
        tracing::info!("set_chain_node: {set_chain_node:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_protocol_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let chain_code = "tron";
        let set_chain_node = wallet_manager.get_protocol_list(chain_code).await;
        tracing::info!("get_protocol_list: {set_chain_node:?}");

        let res = wallet_utils::serde_func::serde_to_string(&set_chain_node).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_hot_chain_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let res = wallet_manager.get_hot_chain_list().await;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_chains() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let res = wallet_manager.sync_chains().await;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_wallet_chain_data() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, test_params) = get_manager().await?;
        let res = wallet_manager
            .sync_wallet_chain_data(&test_params.create_wallet_req.wallet_password)
            .await;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }
}
