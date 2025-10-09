use crate::{
    api::ReturnType, manager::WalletManager,
    messaging::mqtt::topics::api_wallet::cmd::address_allock::AddressAllockType,
    request::api_wallet::account::CreateApiAccountReq,
    response_vo::api_wallet::account::ApiAccountInfos,
    service::api_wallet::account::ApiAccountService,
};

impl WalletManager {
    pub async fn get_api_account_list(
        &self,
        wallet_address: &str,
        account_id: u32,
    ) -> ReturnType<ApiAccountInfos> {
        ApiAccountService::new(self.ctx)
            .list_api_accounts(wallet_address, Some(account_id), None)
            .await
    }

    pub async fn create_api_account(&self, req: CreateApiAccountReq) -> ReturnType<()> {
        ApiAccountService::new(self.ctx)
            .create_account(
                &req.wallet_address,
                &req.wallet_password,
                req.indices,
                &req.name,
                req.is_default_name,
                req.api_wallet_type,
            )
            .await
    }

    #[allow(unused)]
    pub async fn expand_address(
        &self,
        address_allock_type: AddressAllockType,
        chain_code: &str,
        index: Option<i32>,
        uid: &str,
        number: u32,
        serial_no: &str,
    ) -> ReturnType<()> {
        ApiAccountService::new(self.ctx)
            .expand_address(address_allock_type, chain_code, index, uid, number, serial_no)
            .await
    }

    pub async fn get_api_account_private_key(
        &self,
        address: &str,
        chain_code: &str,
        password: &str,
    ) -> ReturnType<String> {
        let res = ApiAccountService::new(self.ctx)
            .get_account_private_key(address, chain_code, password)
            .await?;
        Ok(res.to_string())
    }

    pub async fn address_used(&self, chain_code: &str, index: i32, uid: &str) -> ReturnType<()> {
        ApiAccountService::new(self.ctx).address_used(chain_code, index, uid).await
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

        let wallet_address = "0xDA32fc1346Fa1DF9719f701cbdd6855c901027C1";
        let wallet_password = "q1111111";
        let index = vec![9, 10];
        let name = "666";
        let is_default_name = true;
        let api_wallet_type = ApiWalletType::Withdrawal;

        let req = CreateApiAccountReq::new(
            wallet_address,
            wallet_password,
            index,
            name,
            is_default_name,
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

        let res = wallet_manager.get_api_account_private_key(address, chain_code, password).await;
        tracing::info!("res: {res:?}");

        Ok(())
    }

    #[tokio::test]
    async fn test_address_used() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        let chain_code = "tron";

        let res = wallet_manager
            .address_used(
                chain_code,
                1,
                "eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c",
            )
            .await;
        tracing::info!("res: {res:?}");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_api_account_list() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        // let chain_code = "tron";

        let res = wallet_manager
            .get_api_account_list("0x01a68baa7523f16D64AD63d8a82A40e838170b5b", 0)
            .await
            .unwrap();
        let res = serde_json::to_string(&res).unwrap();
        tracing::info!("res: {res:?}");

        Ok(())
    }
}
