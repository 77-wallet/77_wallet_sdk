use wallet_transport_backend::{
    request::api_wallet::strategy::ChainConfig,
    response_vo::api_wallet::strategy::{CollectionStrategyResp, WithdrawStrategyResp},
};

use crate::{api::ReturnType, service::api_wallet::strategy::StrategyService};

impl crate::WalletManager {
    pub async fn get_collection_strategy(&self, uid: &str) -> ReturnType<CollectionStrategyResp> {
        StrategyService::new().get_collection_strategy(uid).await?.into()
    }

    pub async fn update_collection_strategy(
        &self,
        uid: &str,
        threshold: f64,
        chain_config: Vec<ChainConfig>,
    ) -> ReturnType<()> {
        StrategyService::new()
            .update_collection_strategy(uid, threshold, chain_config)
            .await?
            .into()
    }

    pub async fn get_withdrawal_strategy(&self, uid: &str) -> ReturnType<WithdrawStrategyResp> {
        StrategyService::new().get_withdraw_strategy(uid).await?.into()
    }

    pub async fn update_withdrawal_strategy(
        &self,
        uid: &str,
        threshold: f64,
        chain_config: Vec<ChainConfig>,
    ) -> ReturnType<()> {
        StrategyService::new().update_withdraw_strategy(uid, threshold, chain_config).await?.into()
    }
}

#[cfg(test)]
mod test {
    // #[tokio::test]
    // async fn test_create_api_account() -> Result<()> {
    //     wallet_utils::init_test_log();
    //     // 修改返回类型为Result<(), anyhow::Error>
    //     let (wallet_manager, _test_params) = get_manager().await?;

    //     let wallet_address = "0x6F0e4B9F7dD608A949138bCE4A29e076025b767F";
    //     let wallet_password = "q1111111";
    //     let index = None;
    //     let name = "666";
    //     let is_default_name = true;
    //     let api_wallet_type = ApiWalletType::SubAccount;

    //     let req = CreateApiAccountReq::new(
    //         wallet_address,
    //         wallet_password,
    //         index,
    //         name,
    //         is_default_name,
    //         api_wallet_type,
    //     );
    //     let res = wallet_manager.create_api_account(req).await;
    //     tracing::info!("res: {res:?}");
    //     Ok(())
    // }
}
