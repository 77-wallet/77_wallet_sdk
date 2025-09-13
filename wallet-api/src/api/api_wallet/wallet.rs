use wallet_database::entities::api_wallet::ApiWalletEntity;
pub use wallet_database::entities::api_wallet::ApiWalletType;

use crate::{api::ReturnType, service::api_wallet::wallet::ApiWalletService};

impl crate::WalletManager {
    pub async fn get_api_wallet_list(&self) -> ReturnType<Vec<ApiWalletEntity>> {
        vec![].into()
    }

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
        ApiWalletService::new()
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

    pub async fn bind_merchant(&self, key: &str, merchain_id: &str, recharge_uid: &str, withdrawal_uid: &str) -> ReturnType<()> {
        ApiWalletService::new().bind_merchant(key, merchain_id, recharge_uid).await?.into()
    }

    pub async fn unbind_merchant(&self, recharge_uid: &str, withdrawal_uid: &str) -> ReturnType<()> {
        ApiWalletService::new().unbind_merchant(recharge_uid).await?.into()
    }

    pub async fn edit_api_wallet_name(
        &self,
        wallet_name: &str,
        wallet_address: &str,
    ) -> ReturnType<()> {
        ApiWalletService::new().edit_wallet_name(wallet_address, wallet_name).await?.into()
    }

    pub async fn set_passwd_cache(&self, wallet_password: &str) -> ReturnType<()> {
        ApiWalletService::new().set_passwd_cache(wallet_password).await?.into()
    }
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
        let salt = "";
        let wallet_name = "api_wallet";
        let account_name = "ccccc";
        let is_default_name = true;
        let wallet_password = "q1111111";
        let invite_code = None;
        let api_wallet_type = ApiWalletType::SubAccount;
        // let api_wallet_type = ApiWalletType::Withdrawal;
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

    #[tokio::test]
    async fn test_bind_merchain() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let key = "app_id";
        let merchain_id = "test_merchain";
        let uid = "04de3a5eff89883fecd1469fbc7621f37122c83d6680b95ad5c67cd9a141cd4e";

        let res = wallet_manager.bind_merchant(key, merchain_id, uid, uid).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }
}
