use std::collections::HashMap;

use crate::{
    api::ReturnType, manager::WalletManager, response_vo::chain::ChainAssets,
    service::api_wallet::chain::ApiChainService,
};

impl WalletManager {
    pub async fn get_api_chain_list(
        &self,
        wallet_address: &str,
        account_id: u32,
        chain_list: HashMap<String, String>,
    ) -> ReturnType<Vec<ChainAssets>> {
        ApiChainService::new()
            .get_chain_assets_list(wallet_address, Some(account_id), chain_list)
            .await
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use anyhow::Result;

    use crate::test::env::get_manager;

    #[tokio::test]
    async fn test_get_api_chain_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let wallet_address = "0x0d8B30ED6837b2EF0465Be9EE840700A589eaDB6";
        let account_id = 1;
        let mut chain_list = HashMap::new();
        chain_list.insert("tron".to_string(), "".to_string());
        let res = wallet_manager
            .get_api_chain_list(wallet_address, account_id, chain_list)
            .await
            .unwrap();
        let res = serde_json::to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }
}
