use crate::{
    api::ReturnType,
    manager::WalletManager,
    response_vo::app::{GetConfigRes, GlobalMsg},
    service::app::AppService,
};
use wallet_database::entities::{
    api_wallet::ApiWalletType,
    config::{ConfigEntity, MinValueSwitchConfig},
};
use wallet_transport_backend::response_vo::app::{
    AppVersionRes, GetFiatRes, GetOfficialWebsiteRes,
};

impl WalletManager {
    pub async fn app_install(&self, sn: &str, device_type: &str, channel: &str) -> ReturnType<()> {
        AppService::new(self.repo_factory.resource_repo())
            .app_install_save(sn, device_type, channel)
            .await
    }

    // app版本检测接口
    pub async fn check_version(&self, r#type: &str) -> ReturnType<AppVersionRes> {
        AppService::new(self.repo_factory.resource_repo()).check_version(r#type).await
    }

    pub async fn set_currency(&self, fiat: &str) -> ReturnType<()> {
        AppService::new(self.repo_factory.resource_repo()).set_fiat(fiat).await
    }

    pub async fn set_language(&self, language: &str) -> ReturnType<()> {
        AppService::new(self.repo_factory.resource_repo()).language_init(language).await
    }

    pub async fn set_app_id(&self, app_id: &str) -> ReturnType<()> {
        AppService::new(self.repo_factory.resource_repo()).set_app_id(app_id).await
    }

    pub async fn get_fiat(&self) -> ReturnType<GetFiatRes> {
        AppService::new(self.repo_factory.resource_repo()).get_fiat().await
    }

    pub async fn get_official_website(&self) -> ReturnType<GetOfficialWebsiteRes> {
        AppService::new(self.repo_factory.resource_repo()).get_official_website().await
    }

    pub async fn get_config(&self) -> ReturnType<GetConfigRes> {
        AppService::new(self.repo_factory.resource_repo()).get_config().await
    }

    pub async fn get_unread_status(&self) -> ReturnType<crate::response_vo::app::UnreadCount> {
        AppService::new(self.repo_factory.resource_repo()).get_unread_status().await
    }

    /// Platform Energy Subsidy Switch Configuration
    pub async fn delegate_witch(&self) -> ReturnType<bool> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let res = backend.delegate_is_open().await?;
        Ok(res)
    }

    pub async fn upload_log_file(
        &self,
        req: Vec<crate::request::app::UploadLogFileReq>,
    ) -> ReturnType<()> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        AppService::new(repo).upload_log_file(req).await
    }

    pub async fn mqtt_subscribe(&self, topics: Vec<String>, qos: Option<u8>) -> ReturnType<()> {
        AppService::new(self.repo_factory.resource_repo()).mqtt_subscribe(topics, qos).await
    }

    pub async fn mqtt_unsubscribe(&self, topics: Vec<String>) -> ReturnType<()> {
        AppService::new(self.repo_factory.resource_repo())
            .mqtt_unsubscribe_unsubscribe(topics)
            .await
    }

    pub async fn get_configs(&self) -> ReturnType<Vec<ConfigEntity>> {
        AppService::new(self.repo_factory.resource_repo()).get_configs().await
    }

    pub async fn set_config(&self, key: String, value: String) -> ReturnType<ConfigEntity> {
        AppService::new(self.repo_factory.resource_repo()).set_config(key, value).await
    }

    pub async fn set_min_value_config(
        &self,
        symbol: String,
        amount: f64,
        switch: bool,
    ) -> ReturnType<MinValueSwitchConfig> {
        AppService::new(self.repo_factory.resource_repo())
            .set_min_value_config(symbol, amount, switch)
            .await
    }

    pub async fn get_min_value_config(
        &self,
        symbol: String,
    ) -> ReturnType<Option<MinValueSwitchConfig>> {
        AppService::new(self.repo_factory.resource_repo()).get_min_value_config(symbol).await
    }

    // app 自己请求后端
    pub async fn request(&self, endpoint: String, body: String) -> ReturnType<serde_json::Value> {
        AppService::new(self.repo_factory.resource_repo()).request_backend(&endpoint, body).await
    }

    // 全局的msg
    pub async fn global_msg(&self) -> ReturnType<GlobalMsg> {
        AppService::new(self.repo_factory.resource_repo()).global_msg().await
    }

    /// 设置邀请码
    pub async fn set_invite_code(&self, invite_code: Option<String>) -> ReturnType<()> {
        AppService::new(self.repo_factory.resource_repo()).set_invite_code(invite_code).await
    }

    pub async fn backend_config(&self) -> ReturnType<std::collections::HashMap<String, String>> {
        AppService::new(self.repo_factory.resource_repo()).backend_config().await
    }

    pub async fn set_wallet_type(&self, wallet_type: ApiWalletType) -> ReturnType<()> {
        AppService::new(self.repo_factory.resource_repo()).set_wallet_type(wallet_type).await
    }

    pub async fn get_current_wallet_type(&self) -> ApiWalletType {
        AppService::new(self.repo_factory.resource_repo()).get_current_wallet_type().await
    }
}

#[cfg(test)]
mod test {
    use crate::test::env::get_manager;
    use anyhow::Result;

    #[tokio::test]
    async fn test_set_language() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let res = wallet_manager.set_language("ENGLISH").await?;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_official_website() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let res = wallet_manager.get_official_website().await?;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_config() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let res = wallet_manager.get_config().await?;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_unread_status() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let res = wallet_manager.get_unread_status().await?;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    pub async fn check_version() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        // let r#type = Some("android_google_shop".to_string());
        let r#type = "android_google_shop";
        let res = wallet_manager.check_version(r#type).await?;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    pub async fn set_currency() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let currency = "CNY";
        let res = wallet_manager.set_currency(currency).await?;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_delegate_switch() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let res = wallet_manager.delegate_witch().await?;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    // #[tokio::test]
    // async fn test_upload_log_file() -> Result<()> {
    //     wallet_utils::init_test_log();
    //     // 修改返回类型为Result<(), anyhow::Error>
    //     let (wallet_manager, _test_params) = get_manager().await?;
    //     let res = wallet_manager
    //         .upload_log_file(src_file_path, dst_file_name)
    //         .await;
    //     let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    //     tracing::info!("res: {res:?}");
    //     Ok(())
    // }

    #[tokio::test]
    async fn test_set_app_id() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let res = wallet_manager.set_app_id("65767").await?;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_set_invite_code() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        let res = wallet_manager.set_invite_code(Some("43434".to_string())).await?;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }
}
