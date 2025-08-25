use crate::api::ReturnType;
use crate::service::api_wallet::withdraw::WithdrawService;
use wallet_database::entities::api_withdraw::ApiWithdrawEntity;

impl crate::WalletManager {
    pub async fn get_withdraw_order_list(&self) -> ReturnType<Vec<ApiWithdrawEntity>> {
        WithdrawService::new(self.repo_factory.resource_repo())
            .get_withdraw_order_list()
            .await?
            .into()
    }

    pub async fn sign_withdrawal_order(&self, order_id: &str) -> ReturnType<()> {
        WithdrawService::new(self.repo_factory.resource_repo())
            .sign_withdrawal_order(order_id, 1)
            .await?
            .into()
    }

    pub async fn reject_withdrawal_order(&self, order_id: &str) -> ReturnType<()> {
        WithdrawService::new(self.repo_factory.resource_repo())
            .reject_withdrawal_order(order_id, 2)
            .await?
            .into()
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
