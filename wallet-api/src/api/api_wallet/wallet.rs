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
    ) -> ReturnType<crate::response_vo::standard_wallet::wallet::GetPhraseRes> {
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
        // let salt = "q6666669";
        let salt = "r0000005";
        // let salt = "r0000002";
        // let salt = "r77777777";
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
        // let salt = "q7777781";
        let salt = "w0000005";
        // let salt = "w0000002";
        // let salt = "q7777777";
        // let binding_address = Some("0xF1C1FE41b1c50188faFDce5f21638e1701506f1b");
        // let binding_address = Some("0x7092d3B98B177e630efbA09c047D2bd448608Dda");
        // let binding_address = Some("0x806b94a00D6a4e415739D54D476832Adf432f229");
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

    #[tokio::test]
    async fn test_import_sub_account_api_wallet() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let language_code = 1;
        let phrase = &_test_params.create_wallet_req.phrase;

        // let phrase = "twelve science agree deny swamp square pledge celery person edit fade wasp";
        // let phrase = &"lottery trigger youth daughter note view warm learn devote hair item dress"
        // .to_string();
        // let salt = "7";
        // let salt = "q6666666";
        let salt = "r0000003";
        // let salt = "1234qwer";
        // let salt = "q6666668";
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

        // let phrase =
        //     "sausage basic spray display fresh convince cost there picture chef current guide";
        let wallet_name = "api_wallet";

        let wallet_password = "q1111111";

        let api_wallet_type = ApiWalletType::Withdrawal;
        let invite_code = None;
        // let salt = "10";
        // let salt = "q2222222";
        let salt = "w0000003";
        // let salt = "q7777777";
        // let salt = "q7777780";
        // let salt = "1234qwer";
        // let binding_address = Some("0x17f6a199862FD0ffb2d5C79f3DBBE37597162A24");
        let binding_address = Some("0x1b6c7a238E27590a06bD6f200DA4a8d1b5899d4C");
        // let binding_address = Some("0x7F90ff4374cDFEF97c7Fd546B5E038E06a528166");
        // let binding_address = Some("0x7fD535925bdeB6C1D77475B7f2F8E92475c45D95");
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
        let app_id = "f2a904c3c12e4481bbabb86977c200b3";
        // let merchain_id = "68be7271a7307e042404e276";
        let org_id = "6933cf7a7fec37621a3ffc95";
        let subaccount_uid = "8fa020e0049b10e467fd21ea81b45bf44b88eaec3db8f167173760fc63cf9c90";
        let withdrawal_uid = "f64db1f0796fa815016a067dceb9f912b77ec96ad79dd201534b82e905a1f29a";

        let res =
            wallet_manager.import_bind(sn, org_id, app_id, subaccount_uid, withdrawal_uid).await;
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
        let app_id = "6d7efb1496134170845f11816de23b3c";
        let org_id = "6958fa8ba006d044f3f4c419";
        let subaccount_uid = "823fc91ad98c164d372de036c2e5eec22f47530e7c4ab1c893f653f59260b61f";
        let withdrawal_uid = "587d707c10dee8ed583594ebd3646cb85feb3707d050e2e3a1c1d3d294d887d7";

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
            .physical_delete_api_wallet("0x1b6c7a238E27590a06bD6f200DA4a8d1b5899d4C")
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
