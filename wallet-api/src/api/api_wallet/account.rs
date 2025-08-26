use crate::api::ReturnType;
use crate::request::api_wallet::account::CreateApiAccountReq;
use crate::service::api_wallet::account::ApiAccountService;

impl crate::WalletManager {
    pub async fn create_api_account(&self, req: CreateApiAccountReq) -> ReturnType<()> {
        ApiAccountService::new(self.repo_factory.resource_repo())
            .create_account(
                &req.wallet_address,
                &req.wallet_password,
                req.index,
                &req.name,
                req.is_default_name,
                req.number,
                req.api_wallet_type,
            )
            .await?
            .into()
    }

    pub(crate) async fn upload_allocated_addresses(
        &self,
        wallet_address: &str,
        addresses: Vec<String>,
    ) -> ReturnType<()> {
        ApiAccountService::new(self.repo_factory.resource_repo())
            .upload_allocated_addresses(wallet_address, addresses)
            .await?
            .into()
    }

    pub async fn get_api_account_private_key(
        &self,
        address: &str,
        chain_code: &str,
        password: &str,
    ) -> ReturnType<String> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        ApiAccountService::new(repo)
            .get_account_private_key(address, chain_code, password)
            .await?
            .to_string()
            .into()
    }
}

#[cfg(test)]
mod test {
    use crate::{request::api_wallet::account::CreateApiAccountReq, test::env::get_manager};

    use anyhow::Result;

    use wallet_database::entities::api_wallet::ApiWalletType;

    #[tokio::test]
    async fn test_create_api_account() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let wallet_address = "0x0996dc2A80F35D7075C426bf0Ac6e389e0AB99Fc";
        let wallet_password = "q1111111";
        let index = None;
        let name = "666";
        let is_default_name = true;
        let number = 1;
        let api_wallet_type = ApiWalletType::SubAccount;

        let req = CreateApiAccountReq::new(
            wallet_address,
            wallet_password,
            index,
            name,
            is_default_name,
            number,
            api_wallet_type,
        );
        let res = wallet_manager.create_api_account(req).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_api_account_private_key() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        let address = "1BUttKYoVhXZbAogpHmm2Mm7X8Xtrjn6XH";
        let chain_code = "btc";
        let password = "q1111111";

        let res = wallet_manager
            .get_api_account_private_key(address, chain_code, password)
            .await;
        tracing::info!("res: {res:?}");

        Ok(())
    }
}
