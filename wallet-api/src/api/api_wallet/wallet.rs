use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_transport_backend::response_vo::api_wallet::wallet::{
    QueryUidBindInfoRes, QueryWalletActivationInfoResp,
};

use crate::{
    api::ReturnType, manager::WalletManager, response_vo::api_wallet::wallet::ApiWalletList,
    service::api_wallet::wallet::ApiWalletService,
};

impl WalletManager {
    pub async fn get_api_wallet_list(&self) -> ReturnType<ApiWalletList> {
        ApiWalletService::new(self.ctx).get_api_wallet_list().await
    }

    pub async fn create_api_wallet(
        &self,
        language_code: u8,
        phrase: &str,
        salt: &str,
        wallet_name: &str,
        // account_name: &str,
        // is_default_name: bool,
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
        // account_name: &str,
        // is_default_name: bool,
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
                // account_name,
                // is_default_name,
                wallet_password,
                invite_code,
                api_wallet_type,
                binding_address,
            )
            .await
    }

    pub async fn change_withdrawal_wallet(
        &self,
        recharge_uid: &str,
        withdrawal_uid: &str,
    ) -> ReturnType<()> {
        ApiWalletService::new(self.ctx).change_withdrawal_wallet(recharge_uid, withdrawal_uid).await
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

    pub async fn get_api_phrase(
        &self,
        wallet_address: &str,
        password: &str,
    ) -> ReturnType<crate::response_vo::wallet::GetPhraseRes> {
        ApiWalletService::new(self.ctx).get_phrase(wallet_address, password).await
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

    pub async fn physical_delete_api_wallet(&self, address: &str) -> ReturnType<()> {
        ApiWalletService::new(self.ctx).physical_delete(address).await
    }

    // pub async fn appid_withdrawal_wallet_change(
    //     &self,
    //     withdrawal_uid: &str,
    //     org_app_id: &str,
    // ) -> ReturnType<()> {
    //     ApiWalletService::new(self.ctx)
    //         .appid_withdrawal_wallet_change(withdrawal_uid, org_app_id)
    //         .await
    // }

    // 钱包是否在本设备有效
    pub async fn is_wallet_authorized_on_device(&self, wallet_address: &str) -> ReturnType<bool> {
        ApiWalletService::new(self.ctx).is_wallet_authorized_on_device(wallet_address).await
    }
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
        wallet_manager.init_api_swap().await?;

        let language_code = 1;
        let phrase = &test_params.create_wallet_req.phrase;
        // let salt = "7";
        // let salt = "q3333333";
        let salt = "q6666668";
        let wallet_name = "api_wallet";

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
        // wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;
        let language_code = 1;
        let phrase = &test_params.create_wallet_req.phrase;
        let wallet_name = "api_wallet";

        let wallet_password = "q1111111";

        let api_wallet_type = ApiWalletType::Withdrawal;
        let invite_code = None;
        // let salt = "10";
        // let salt = "q2222222";
        let salt = "q7777781";
        // let binding_address = Some("0xF1C1FE41b1c50188faFDce5f21638e1701506f1b");
        // let binding_address = Some("0x7092d3B98B177e630efbA09c047D2bd448608Dda");
        // let binding_address = Some("0x007d2C90Cf619aDe1b090992f69Dc7394fD21f36");
        let binding_address = None;
        // let binding_address = Some("0x7F90ff4374cDFEF97c7Fd546B5E038E06a528166");
        let res = wallet_manager
            .create_api_wallet(
                language_code,
                phrase,
                salt,
                wallet_name,
                wallet_password,
                invite_code,
                api_wallet_type,
                binding_address,
            )
            .await;
        tracing::info!("create withdrawal wallet res: {res:?}");
        Ok(())
    }
    // 69115152444c0b49fc7b9f3c	AwmCmdAddrExpand	{"data":{"chain":"tron","index":null,"number":"5","serialNo":"tron_88a06da151b1d51c3f9e751ba398be4abb67e816359c849ef66ac0c7bbbd0640","type":"CHA_BATCH","uid":"88a06da151b1d51c3f9e751ba398be4abb67e816359c849ef66ac0c7bbbd0640"},"eventNo":"1987712693663371264","eventType":"3","secret":"jnRkLB2TnTDOLsfqsOGsFlnMyoL4qJcKNeNuaFejctA=","sign":"rajb0qK3NJNnwfhgYvGiT1jw1nL8cREURz4M+d3QZW8fhJRVNb2YknT8qLu2jbfw3FqIrV27Nc6t7dPqz6IqDg==","time":1762742610}	2	111	3	2025-11-10T02:43:31Z	2025-11-13T05:47:42Z	Business error: api wallet error: Api Account error: Expand address not done yet

    #[tokio::test]
    async fn test_import_sub_account_api_wallet() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let language_code = 1;
        let phrase = &_test_params.create_wallet_req.phrase;
        // let phrase = &"lottery trigger youth daughter note view warm learn devote hair item dress"
        // .to_string();
        // let salt = "7";
        // let salt = "q6666666";
        let salt = "q6666668";
        let wallet_name = "api_wallet";

        let wallet_password = "q1111111";
        let invite_code = None;
        let api_wallet_type = ApiWalletType::SubAccount;
        let binding_address = None;
        // let api_wallet_type = ApiWalletType::Withdrawal;
        let res = wallet_manager
            .import_api_wallet(
                language_code,
                phrase,
                salt,
                wallet_name,
                wallet_password,
                invite_code,
                api_wallet_type,
                binding_address,
            )
            .await;
        tracing::info!("create sub wallet res: {res:?}");
        let res: (i64, String) = match res {
            Ok(_) => (0, "success".to_string()),
            Err(e) => e.into(),
        };

        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_import_withdrawal_api_wallet() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let language_code = 1;
        let phrase = &test_params.create_wallet_req.phrase;
        let wallet_name = "api_wallet";

        let wallet_password = "q1111111";

        let api_wallet_type = ApiWalletType::Withdrawal;
        let invite_code = None;
        // let salt = "10";
        // let salt = "q2222222";
        let salt = "q7777777";
        // let binding_address = Some("0x17f6a199862FD0ffb2d5C79f3DBBE37597162A24");
        // let binding_address = None;
        let binding_address = Some("0x7F90ff4374cDFEF97c7Fd546B5E038E06a528166");
        let res = wallet_manager
            .import_api_wallet(
                language_code,
                phrase,
                salt,
                wallet_name,
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
        wallet_manager.init_api_swap().await?;

        let uid = "4080938dda41a016b8c153be34b558345259a4b4116d5a88e004507341164b78";
        let res = wallet_manager.query_uid_bind_info(uid).await.unwrap();
        let res = serde_json::to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_import_bind() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;
        let _ = wallet_manager.set_passwd_cache("q1111111").await;

        let sn = &_test_params.device_req.sn;
        // let key = "M1971511237015650304";
        let app_id = "455f43930e3b432ba3acd51bfb4e1aa4";
        // let merchain_id = "68be7271a7307e042404e276";
        let merchain_id = "68fb31dc6c6e12567646b3fa";
        let subaccount_uid = "87c2274b47f4b93329b9d686dae2c4bc0d96bdc4fd602320a4e87089bda7c915";
        let withdrawal_uid = "4080938dda41a016b8c153be34b558345259a4b4116d5a88e004507341164b78";

        let res = wallet_manager
            .import_bind(sn, merchain_id, app_id, subaccount_uid, withdrawal_uid)
            .await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_scan_bind() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;
        let _ = wallet_manager.set_passwd_cache("q1111111").await;

        // let app_id = "2956f07a24d94fb6b6426abcfeaca2be";
        let app_id = "2d98ad9022ce4a4680e4ac69719cd05a";
        let org_id = "68fb546daa6d73588df4ed27";
        let subaccount_uid = "b806f79d8c0f60a25d40d81f32f546bae2a924d6ecf392df694f6908c6573f36";
        let withdrawal_uid = "1544c91942a23fb8c04c0308c6406c5efefae4c46eb25fba64dd09833f447c9d";

        let res = wallet_manager.scan_bind(app_id, org_id, subaccount_uid, withdrawal_uid).await;
        tracing::info!("res: {res:?}");
        let res: (i64, String) = match res {
            Ok(_) => (0, "success".to_string()),
            Err(e) => e.into(),
        };

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
        wallet_manager.init_api_swap().await?;
        let _ = wallet_manager.set_passwd_cache("q1111111").await;

        // let wallet_address = "0x01a68baa7523f16D64AD63d8a82A40e838170b5b";
        let wallet_address = "0x76c17D35200533Aa9cB326a1A07B75aFBc89fB02";

        let res = wallet_manager.query_wallet_activation_info(wallet_address).await.unwrap();
        let res = serde_json::to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_api_wallet_list() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let _ = wallet_manager.set_passwd_cache("q1111111").await;

        let res = wallet_manager.get_api_wallet_list().await.unwrap();
        let res = serde_json::to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_physical_delete_api_wallet() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        let _ = wallet_manager.set_passwd_cache("q1111111").await;

        let res = wallet_manager
            .physical_delete_api_wallet("0x234bb8664b5a38573Be7116C10c41cd5c7CbcCD9")
            .await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_api_phrase() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        let _ = wallet_manager.set_passwd_cache("q1111111").await;

        let res = wallet_manager
            .get_api_phrase("0x4A0e394b4B8983fF9Db3C1d866bc1b4121345Aa4", "q1111111")
            .await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_set_passwd_cache() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        let res = wallet_manager.set_passwd_cache("q1111111").await;

        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_change_withdrawal_wallet() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;
        let _ = wallet_manager.set_passwd_cache("q1111111").await;

        let recharge_uid = "703dc9ffe712d3ced169cee62c3c9c8118ce822bd00d49650e02df80ba0fcc30";
        let withdrawal_uid = "17931d2265113d34604598200350c0e5eba860af969768c91d5aee7f499c08c1";
        let res =
            wallet_manager.change_withdrawal_wallet(recharge_uid, withdrawal_uid).await.unwrap();
        let res = serde_json::to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_is_wallet_authorized_on_device() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;
        let res = wallet_manager
            .is_wallet_authorized_on_device("0x7F90ff4374cDFEF97c7Fd546B5E038E06a528166")
            .await;
        tracing::info!("res: {res:?}");
        Ok(())
    }
}
