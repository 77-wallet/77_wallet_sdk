use crate::{
    api::ReturnType, manager::WalletManager, service::api_wallet::asset::ApiAssetsService,
};
use wallet_database::entities::api_assets::ApiAssetsEntity;

impl WalletManager {
    pub async fn get_api_assets_list(
        &self,
        wallet_address: &str,
        account_id: Option<u32>,
    ) -> ReturnType<Vec<ApiAssetsEntity>> {
        Ok(vec![])
    }

    // 根据钱包去同步资产
    pub async fn sync_api_assets_by_wallet(
        &self,
        wallet_address: &str,
        account_id: Option<u32>,
        symbol: Vec<String>,
    ) -> ReturnType<()> {
        let res = ApiAssetsService::new()
            .sync_assets_by_wallet_chain(wallet_address, account_id, symbol)
            .await;

        if let Err(e) = res {
            tracing::error!("sync_assets error: {}", e);
            return Err(e);
        }

        Ok(())
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

        let wallet_address = "0xF1C1FE41b1c50188faFDce5f21638e1701506f1b";
        let index = None;

        let res = wallet_manager.sync_api_assets_by_wallet(wallet_address, index, vec![]).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }
}
