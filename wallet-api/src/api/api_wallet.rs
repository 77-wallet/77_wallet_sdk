use wallet_database::entities::api_wallet::ApiWalletType;

use crate::api::ReturnType;
use crate::service::api_wallet::ApiWalletService;

impl crate::WalletManager {
    pub async fn create_api_wallet(
        &self,
        language_code: u8,
        phrase: &str,
        salt: &str,
        wallet_name: &str,
        account_name: &str,
        is_default_name: bool,
        wallet_password: &str,
        invite_code: Option<String>,
        api_wallet_type: ApiWalletType,
    ) -> ReturnType<()> {
        ApiWalletService::new(self.repo_factory.resource_repo())
            .create_wallet(
                language_code,
                phrase,
                salt,
                wallet_name,
                account_name,
                is_default_name,
                wallet_password,
                invite_code,
                api_wallet_type,
            )
            .await?
            .into()
    }

    // pub async fn edit_api_wallet_name(
    //     &self,
    //     wallet_name: &str,
    //     wallet_address: &str,
    // ) -> ReturnType<()> {
    //     WalletService::new(self.repo_factory.resource_repo())
    //         .edit_wallet_name(wallet_name, wallet_address)
    //         .await?
    //         .into()
    // }

    // pub async fn physical_reset_api_wallet(&self) -> ReturnType<()> {
    //     WalletService::new(self.repo_factory.resource_repo())
    //         .physical_reset()
    //         .await?
    //         .into()
    // }

    // pub async fn get_api_wallet_list(
    //     &self,
    //     wallet_address: Option<String>,
    //     chain_code: Option<String>,
    //     account_id: Option<u32>,
    // ) -> ReturnType<Vec<crate::response_vo::wallet::WalletInfo>> {
    //     WalletService::new(self.repo_factory.resource_repo())
    //         .get_wallet_list(wallet_address, chain_code, account_id)
    //         .await?
    //         .into()
    // }

    // pub async fn physical_delete_api_wallet(&self, address: &str) -> ReturnType<()> {
    //     WalletService::new(self.repo_factory.resource_repo())
    //         .physical_delete(address)
    //         .await?
    //         .into()
    // }
}

#[cfg(test)]
mod test {
    use crate::test::env::get_manager;

    use anyhow::Result;

    use wallet_database::entities::api_wallet::ApiWalletType;

    #[tokio::test]
    async fn test_create_api_wallet() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, test_params) = get_manager().await?;

        let language_code = 1;
        let phrase = &test_params.create_wallet_req.phrase;
        let salt = "q1111111";
        let wallet_name = "api_wallet";
        let account_name = "ccccc";
        let is_default_name = true;
        let wallet_password = "q1111111";
        let invite_code = None;
        let api_wallet_type = ApiWalletType::SubAccount;
        let res = wallet_manager
            .create_api_wallet(
                language_code,
                phrase,
                salt,
                wallet_name,
                account_name,
                is_default_name,
                wallet_password,
                invite_code,
                api_wallet_type,
            )
            .await;
        tracing::info!("res: {res:?}");
        Ok(())
    }
}
