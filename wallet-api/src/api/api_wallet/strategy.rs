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
        threshold: f64,
        chain_config: Vec<ChainConfig>,
    ) -> ReturnType<()> {
        StrategyService::new(self.ctx).update_collect_strategy(uid, threshold, chain_config).await
    }

    pub async fn get_collect_strategy(&self, uid: &str) -> ReturnType<CollectionStrategyResp> {
        StrategyService::new(self.ctx).query_collect_strategy(uid).await
    }

    pub async fn update_withdrawal_strategy(
        &self,
        uid: &str,
        threshold: f64,
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

#[cfg(test)]
mod test {
    use crate::test::env::get_manager;
    use anyhow::Result;
    use wallet_transport_backend::request::api_wallet::strategy::{ChainConfig, IndexAndAddress};
    use wallet_types::chain::chain::ChainCode;

    #[tokio::test]
    async fn test_update_collect_strategy() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let uid = "2b3c9d25a6d68fd127a77c4d8fefcb6c2466ac40e5605076ee3e1146f5f66993";
        let threshold = 1.1;
        let chain_config = vec![ChainConfig {
            chain_code: ChainCode::Tron.to_string(),
            normal_address: IndexAndAddress {
                index: None,
                address: "TSdB5jJpdBGZLKHA1CpQeb3S5ZcVF9dceG".to_string(),
                chain_address_type: None,
            },
            risk_address: IndexAndAddress {
                index: None,
                address: "TSdB5jJpdBGZLKHA1CpQeb3S5ZcVF9dceG".to_string(),
                chain_address_type: None,
            },
        }];
        let res = wallet_manager.update_collect_strategy(uid, threshold, chain_config).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_collect_strategy() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        let uid = "eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c";
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

        let uid = "fbed6396c5a6249bb19af98b101701427be4d14a0721fd9258c3e495fb848e35";
        let threshold = 1.1;
        let chain_config = vec![ChainConfig {
            chain_code: ChainCode::Tron.to_string(),
            normal_address: IndexAndAddress {
                index: Some(0),
                address: "TCdNZCKVMsEXvW7tUzAYh3s852mpGMffUj".to_string(),
                chain_address_type: None,
            },
            risk_address: IndexAndAddress {
                index: Some(1),
                address: "TEsdVAqnufo1ciSGd847yTBsFnRqY4mxan".to_string(),
                chain_address_type: None,
            },
        }];
        let res = wallet_manager.update_withdrawal_strategy(uid, threshold, chain_config).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_withdrawal_strategy() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        let uid = "2b607a707cc4f0b4191bce26149e0310302905a59aed4c27b35d6429bfacd5d9";
        let res = wallet_manager.get_withdrawal_strategy(uid).await.unwrap();
        let res = serde_json::to_string(&res).unwrap();
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
