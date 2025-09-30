use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_transport_backend::response_vo::api_wallet::wallet::{
    QueryUidBindInfoRes, QueryWalletActivationInfoResp,
};

use crate::{
    api::ReturnType, manager::WalletManager, response_vo::api_wallet::wallet::ApiWalletInfo,
    service::api_wallet::wallet::ApiWalletService,
};

impl WalletManager {
    pub async fn get_api_wallet_list(
        &self,
        api_wallet_type: ApiWalletType,
    ) -> ReturnType<Vec<ApiWalletInfo>> {
        ApiWalletService::new(self.ctx).get_api_wallet_list(api_wallet_type).await
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
        binding_address: Option<&str>,
    ) -> ReturnType<String> {
        ApiWalletService::new(self.ctx)
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
                binding_address,
            )
            .await
    }

    pub async fn import_api_wallet(
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
        binding_address: Option<&str>,
    ) -> ReturnType<String> {
        ApiWalletService::new(self.ctx)
            .import_wallet(
                language_code,
                phrase,
                salt,
                wallet_name,
                account_name,
                is_default_name,
                wallet_password,
                invite_code,
                api_wallet_type,
                binding_address,
            )
            .await
    }

    /// 查询绑定信息
    pub async fn query_uid_bind_info(&self, uid: &str) -> ReturnType<QueryUidBindInfoRes> {
        ApiWalletService::new(self.ctx).query_uid_bind_info(uid).await
    }

    // pub async fn get_wallet_address(&self) -> ReturnType<()> {
    //     ApiWalletService::new(self.repo_factory.resource_repo())
    //         .get_wallet_address(key, merchain_id, uid)
    //         .await?
    //         .into()
    // }

    pub async fn wallet_list(&self) -> ReturnType<Vec<ApiWalletInfo>> {
        ApiWalletService::new(self.ctx).get_api_wallet_list(ApiWalletType::Withdrawal).await
    }

    pub async fn scan_bind(
        &self,
        org_app_id: &str,
        merchain_id: &str,
        recharge_uid: &str,
        withdrawal_uid: &str,
    ) -> ReturnType<()> {
        ApiWalletService::new(self.ctx)
            .scan_bind(org_app_id, merchain_id, recharge_uid, withdrawal_uid)
            .await
    }

    pub async fn import_bind(
        &self,
        sn: &str,
        org_id: &str,
        app_id: &str,
        recharge_uid: &str,
        withdrawal_uid: &str,
    ) -> ReturnType<()> {
        ApiWalletService::new(self.ctx)
            .import_bind(sn, org_id, app_id, recharge_uid, withdrawal_uid)
            .await
    }

    pub async fn unbind_merchant(
        &self,
        recharge_uid: &str,
        withdrawal_uid: &str,
    ) -> ReturnType<()> {
        ApiWalletService::new(self.ctx).unbind_merchant(recharge_uid, withdrawal_uid).await
    }

    pub async fn edit_api_wallet_name(
        &self,
        wallet_name: &str,
        wallet_address: &str,
    ) -> ReturnType<()> {
        ApiWalletService::new(self.ctx).edit_wallet_name(wallet_address, wallet_name).await
    }

    pub async fn set_passwd_cache(&self, wallet_password: &str) -> ReturnType<()> {
        ApiWalletService::new(self.ctx).set_passwd_cache(wallet_password).await
    }

    pub async fn query_wallet_activation_info(
        &self,
        wallet_address: &str,
    ) -> ReturnType<QueryWalletActivationInfoResp> {
        ApiWalletService::new(self.ctx).query_wallet_activation_info(wallet_address).await
    }

    // pub async fn physical_reset_api_wallet(&self) -> ReturnType<()> {
    //     WalletService::new(self.repo_factory.resource_repo())
    //         .physical_reset()
    //         .await?
    //         .into()
    // }

    // pub async fn physical_delete_api_wallet(&self, address: &str) -> ReturnType<()> {
    //     WalletService::new(self.repo_factory.resource_repo())
    //         .physical_delete(address)
    //         .await?
    //         .into()
    // }

    // pub async fn appid_withdrawal_wallet_change(
    //     &self,
    //     withdrawal_uid: &str,
    //     org_app_id: &str,
    // ) -> ReturnType<()> {
    //     ApiWalletService::new(self.ctx)
    //         .appid_withdrawal_wallet_change(withdrawal_uid, org_app_id)
    //         .await
    // }
}

#[cfg(test)]
mod test {
    use crate::test::env::get_manager;

    use anyhow::Result;

    use wallet_database::entities::api_wallet::ApiWalletType;

    #[tokio::test]
    async fn test_create_subaccount_wallet() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, test_params) = get_manager().await?;

        let language_code = 1;
        let phrase = &test_params.create_wallet_req.phrase;
        let salt = "5";
        let wallet_name = "api_wallet";
        let account_name = "ccccc";
        let is_default_name = true;
        let wallet_password = "q1111111";
        let api_wallet_type = ApiWalletType::SubAccount;
        let binding_address = None;
        let invite_code = None;
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
                binding_address,
            )
            .await;
        tracing::info!("create sub wallet res: {res:?}");

        Ok(())
    }

    #[tokio::test]
    async fn test_create_withdrawal_wallet() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, test_params) = get_manager().await?;

        let language_code = 1;
        let phrase = &test_params.create_wallet_req.phrase;
        let wallet_name = "api_wallet";
        let account_name = "ccccc";
        let is_default_name = true;
        let wallet_password = "q1111111";

        let api_wallet_type = ApiWalletType::Withdrawal;
        let invite_code = None;
        let salt = "6";
        // let binding_address = Some("0xF1C1FE41b1c50188faFDce5f21638e1701506f1b");
        // let binding_address = Some("0x7092d3B98B177e630efbA09c047D2bd448608Dda");
        let binding_address = Some("0x007d2C90Cf619aDe1b090992f69Dc7394fD21f36");
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
                binding_address,
            )
            .await;
        tracing::info!("create withdrawal wallet res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_import_sub_account_api_wallet() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, test_params) = get_manager().await?;

        let language_code = 1;
        let phrase = &test_params.create_wallet_req.phrase;
        let salt = "3";
        let wallet_name = "api_wallet";
        let account_name = "ccccc";
        let is_default_name = true;
        let wallet_password = "q1111111";
        let invite_code = None;
        let api_wallet_type = ApiWalletType::Withdrawal;
        let binding_address = None;
        // let api_wallet_type = ApiWalletType::Withdrawal;
        let res = wallet_manager
            .import_api_wallet(
                language_code,
                phrase,
                salt,
                wallet_name,
                account_name,
                is_default_name,
                wallet_password,
                invite_code,
                api_wallet_type,
                binding_address,
            )
            .await;
        tracing::info!("create sub wallet res: {res:?}");

        Ok(())
    }

    #[tokio::test]
    async fn test_import_withdrawal_api_wallet() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, test_params) = get_manager().await?;

        let language_code = 1;
        let phrase = &test_params.create_wallet_req.phrase;
        let wallet_name = "api_wallet";
        let account_name = "ccccc";
        let is_default_name = true;
        let wallet_password = "q1111111";

        let api_wallet_type = ApiWalletType::Withdrawal;
        let invite_code = None;
        let salt = "qwer1234";
        let binding_address = Some("0xdfdD0ada5312DD44c212Ba30c867412BbC7B6A2e");
        let res = wallet_manager
            .import_api_wallet(
                language_code,
                phrase,
                salt,
                wallet_name,
                account_name,
                is_default_name,
                wallet_password,
                invite_code,
                api_wallet_type,
                binding_address,
            )
            .await;
        tracing::info!("create withdrawal wallet res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_query_uid_bind_info() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;

        let uid = "eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c";
        let res = wallet_manager.query_uid_bind_info(uid).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_import_bind() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;

        let sn = &_test_params.device_req.sn;
        let key = "68c27dfaa06b0c05e37c5e86";
        let merchain_id = "68be7271a7307e042404e276";
        let subaccount_uid = "eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c";
        let withdrawal_uid = "e6de8afd756e7cb81a3d965f959c896738ed07cebc919c7f96c97fc6069ad44f";

        let res =
            wallet_manager.import_bind(sn, merchain_id, key, subaccount_uid, withdrawal_uid).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_scan_bind() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let _ = wallet_manager.set_passwd_cache("q1111111").await;

        let key = "M1971511237015650304";
        let merchain_id = "68be7271a7307e042404e276";
        let subaccount_uid = "bf6e56761f4a838bd7bdbef5ba3071bf36d3a588a5176fb58e3225f2758ce805";
        let withdrawal_uid = "fbed6396c5a6249bb19af98b101701427be4d14a0721fd9258c3e495fb848e35";

        let res = wallet_manager.scan_bind(key, merchain_id, subaccount_uid, withdrawal_uid).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    // #[tokio::test]
    // async fn test_appid_withdrawal_wallet_change() -> Result<()> {
    //     wallet_utils::init_test_log();
    //     // 修改返回类型为Result<(), anyhow::Error>
    //     let (wallet_manager, _test_params) = get_manager().await?;
    //     let _ = wallet_manager.set_passwd_cache("q1111111").await;

    //     let key = "68c27dfaa06b0c05e37c5e86";
    //     let withdrawal_uid = "e6de8afd756e7cb81a3d965f959c896738ed07cebc919c7f96c97fc6069ad44f";

    //     let res = wallet_manager.appid_withdrawal_wallet_change(withdrawal_uid, key).await;
    //     tracing::info!("res: {res:?}");
    //     Ok(())
    // }

    #[tokio::test]
    async fn test_query_wallet_activation_info() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let _ = wallet_manager.set_passwd_cache("q1111111").await;

        let wallet_address = "0x6d907850763996AbfF4D310AA67a7D9Ce862CF91";

        let res = wallet_manager.query_wallet_activation_info(wallet_address).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }
}
