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

        let wallet_address = "0x6F0e4B9F7dD608A949138bCE4A29e076025b767F";
        let wallet_password = "q1111111";
        let index = None;
        let name = "666";
        let is_default_name = true;
        let number = 3;
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
}
