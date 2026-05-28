use crate::{
    api::ReturnType, manager::WalletManager, service::api_wallet::strategy::StrategyService,
};
use wallet_transport_backend::{
    request::api_wallet::strategy::ChainConfig,
    response_vo::api_wallet::strategy::{CollectionStrategyResp, WithdrawStrategyResp},
};

impl WalletManager {
    pub async fn update_collect_strategy(
        &self,
        uid: &str,
        threshold: u32,
        chain_config: Vec<ChainConfig>,
    ) -> ReturnType<()> {
        tracing::info!(
            "update_collect_strategy: uid={}, threshold={}, chain_config={:?}",
            uid,
            threshold,
            chain_config
        );
        StrategyService::new(self.ctx).update_collect_strategy(uid, threshold, chain_config).await
    }

    pub async fn get_collect_strategy(&self, uid: &str) -> ReturnType<CollectionStrategyResp> {
        StrategyService::new(self.ctx).query_collect_strategy(uid).await
    }

    pub async fn update_withdrawal_strategy(
        &self,
        uid: &str,
        threshold: u32,
        chain_config: Vec<ChainConfig>,
    ) -> ReturnType<()> {
        StrategyService::new(self.ctx)
            .update_withdrawal_strategy(uid, threshold, chain_config)
            .await
    }

    pub async fn get_withdrawal_strategy(&self, uid: &str) -> ReturnType<WithdrawStrategyResp> {
        StrategyService::new(self.ctx).query_withdrawal_strategy(uid).await
    }

    pub async fn query_api_wallet_configs(&self) -> ReturnType<serde_json::Value> {
        StrategyService::new(self.ctx).query_api_wallet_configs().await
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod test {
    use crate::testkit::env::get_manager;
    use anyhow::Result;
    use wallet_transport_backend::request::api_wallet::strategy::{ChainConfig, IndexAndAddress};
    use wallet_types::chain::chain::ChainCode;

    #[tokio::test]
    async fn test_update_collect_strategy() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let uid = "823fc91ad98c164d372de036c2e5eec22f47530e7c4ab1c893f653f59260b61f";

        let threshold = 1;
        let chain_config = vec![ChainConfig {
            chain_code: ChainCode::Tron.to_string(),
            chain_address_type: None,
            normal_address: IndexAndAddress {
                index: Some(0),
                address: "TW6h166qfNfibxgovAnVyDDMNV1BFXp5A5".to_string(),
            },
            risk_address: IndexAndAddress {
                index: Some(1),
                address: "THLja2cJJxjbn4cUZZq6BRX8QHK1sxFbT4".to_string(),
            },
        }];
        let res = wallet_manager.update_collect_strategy(uid, threshold, chain_config).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_update_collect_strategy_2() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let uid = "ef98e62f7057e2c6cee9314ee017875b283dccaaeeeabc9370f8afa7a3a5e186";

        let res = wallet_manager.get_collect_strategy(uid).await.unwrap();

        let threshold = 1;
        let chain_config = res.chain_configs;
        let res = wallet_manager.update_collect_strategy(uid, threshold, chain_config).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_collect_strategy() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;
        let uid = "ef98e62f7057e2c6cee9314ee017875b283dccaaeeeabc9370f8afa7a3a5e186";
        let res = wallet_manager.get_collect_strategy(uid).await.unwrap();
        let res = serde_json::to_string(&res).unwrap();
        tracing::info!("res: {res:?}");

        Ok(())
    }

    #[tokio::test]
    async fn test_update_withdrawal_strategy() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let uid = "e813253c11240023729a033feaa4b271b5e9a2a7e03df0464438e1b3b1bf2fb2";
        let threshold = 1;
        let chain_config = vec![ChainConfig {
            chain_code: ChainCode::Tron.to_string(),
            chain_address_type: Some("TRON".to_string()),
            normal_address: IndexAndAddress {
                index: Some(0),
                address: "TLXdEp1kaVx4ePKpZmXqaU8hBnxsvYUoxf".to_string(),
            },
            risk_address: IndexAndAddress {
                index: Some(0),
                address: "TLXdEp1kaVx4ePKpZmXqaU8hBnxsvYUoxf".to_string(),
            },
        }];
        let res = wallet_manager.update_withdrawal_strategy(uid, threshold, chain_config).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_withdrawal_strategy() -> Result<()> {
        wallet_utils::log::init_log_with_level(tracing::Level::INFO);
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;
        let uid = "5bdb1b748bb617d6683f57565b1493cfa5f9e45f3086daf265ca2e0cd325c15e";
        let res = wallet_manager.get_withdrawal_strategy(uid).await.unwrap();
        let res = serde_json::to_string(&res).unwrap();
        tracing::info!("res: {res:?}");

        Ok(())
    }

    #[tokio::test]
    async fn test_update_withdrawal_strategy_2() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let uid = "5bdb1b748bb617d6683f57565b1493cfa5f9e45f3086daf265ca2e0cd325c15e";

        let res = wallet_manager.get_withdrawal_strategy(uid).await.unwrap();

        let threshold = 5;
        let chain_config = res.chain_configs;
        let res = wallet_manager.update_withdrawal_strategy(uid, threshold, chain_config).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }
    #[tokio::test]
    async fn test_query_api_wallet_configs() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        let res = wallet_manager.query_api_wallet_configs().await.unwrap();
        let res = serde_json::to_string(&res).unwrap();
        tracing::info!("res: {res:?}");

        Ok(())
    }
}
