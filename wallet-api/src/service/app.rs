use wallet_database::{
    dao::config::ConfigDao,
    entities::config::{
        config_key::{BLOCK_BROWSER_URL_LIST, LANGUAGE},
        ConfigEntity, MinValueSwitchConfig,
    },
    repositories::{
        announcement::AnnouncementRepoTrait, device::DeviceRepoTrait,
        system_notification::SystemNotificationRepoTrait, wallet::WalletRepoTrait,
    },
};
use wallet_transport_backend::{
    request::{AppInstallSaveReq, VersionViewReq},
    response_vo::app::{AppVersionRes, GetFiatRes, GetOfficialWebsiteRes},
};

use crate::{
    domain::app::config::ConfigDomain,
    infrastructure::task_queue::{
        BackendApiTask, BackendApiTaskData, InitializationTask, Task, Tasks,
    },
    response_vo::app::GetConfigRes,
};

pub struct AppService<T> {
    repo: T,
    // keystore: wallet_keystore::Keystore
}

impl<
        T: WalletRepoTrait + DeviceRepoTrait + AnnouncementRepoTrait + SystemNotificationRepoTrait,
    > AppService<T>
{
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn get_official_website(self) -> Result<GetOfficialWebsiteRes, crate::ServiceError> {
        let config = crate::app_state::APP_STATE.read().await;

        let official_website = config.get_official_website();
        Ok(GetOfficialWebsiteRes { official_website })
    }

    pub async fn get_config(self) -> Result<GetConfigRes, crate::ServiceError> {
        let config = crate::app_state::APP_STATE.read().await;
        // if config.url().official_website.is_none() {
        //     let official_website = self.app_domain.get_official_website().await.ok();
        //     if let Some(official_website) = official_website {
        //         config.set_official_website(official_website.official_website);
        //     }
        // };
        let url = config.url().clone();
        drop(config);
        if url.block_browser_url_list.is_empty() {
            ConfigDomain::init_block_browser_url_list().await?;
        }
        if url.official_website.is_none() {
            ConfigDomain::init_official_website().await?;
        }
        if url.app_download_qr_code_url.is_none() {
            ConfigDomain::init_app_install_download_url().await?;
        }
        let mut tx = self.repo;
        let wallet_list = tx.wallet_list().await?;
        let device_info = tx.get_device_info().await?;
        let unread_announcement_count = AnnouncementRepoTrait::count_unread_status(&mut tx).await?;
        let unread_system_notification_count =
            SystemNotificationRepoTrait::count_unread_status(&mut tx).await?;

        let config = crate::app_state::APP_STATE.read().await;
        Ok(GetConfigRes {
            fiat: config.currency().to_string(),
            language: config.language().to_string(),
            wallet_list,
            device_info,
            url: config.url().clone(),
            unread_count: crate::response_vo::app::UnreadCount {
                system_notification: unread_system_notification_count,
                announcement: unread_announcement_count,
            },
        })
    }

    pub async fn get_unread_status(
        self,
    ) -> Result<crate::response_vo::app::UnreadCount, crate::ServiceError> {
        let mut tx = self.repo;
        let unread_announcement_count = AnnouncementRepoTrait::count_unread_status(&mut tx).await?;
        let unread_system_notification_count =
            SystemNotificationRepoTrait::count_unread_status(&mut tx).await?;
        Ok(crate::response_vo::app::UnreadCount {
            system_notification: unread_system_notification_count,
            announcement: unread_announcement_count,
        })
    }

    pub async fn language_init(self, language: &str) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;

        let val = wallet_database::entities::config::Language::new(language);
        ConfigDomain::set_config(LANGUAGE, &val.to_json_str()?).await?;
        let device_info = tx.get_device_info().await?;
        if let Some(device_info) = device_info {
            let client_id = crate::domain::app::DeviceDomain::client_id_by_device(&device_info)?;

            let language_req = wallet_transport_backend::request::LanguageInitReq {
                client_id,
                lan: language.to_string(),
            };
            let language_init_task_data = BackendApiTaskData::new(
                wallet_transport_backend::consts::endpoint::LANGUAGE_INIT,
                &language_req,
            )?;
            Tasks::new()
                .push(Task::BackendApi(BackendApiTask::BackendApi(
                    language_init_task_data,
                )))
                .push(Task::Initialization(InitializationTask::PullAnnouncement))
                .send()
                .await?;
            let mut config = crate::app_state::APP_STATE.write().await;
            config.set_language(language);
        }

        Ok(())
    }

    // pub async fn set_config(self, language: SetConfigReq) -> Result<(), crate::ServiceError> {
    //     let mut tx = self.repo;

    //     Ok(())
    // }

    pub async fn check_version(self, r#type: &str) -> Result<AppVersionRes, crate::ServiceError> {
        let req = VersionViewReq::new(r#type);
        let backend = crate::manager::Context::get_global_backend_api()?;
        let res = backend.version_view(req).await?;
        Ok(res)
    }

    // fiat  = CNY
    pub async fn set_fiat(&mut self, fiat: &str) -> Result<(), crate::ServiceError> {
        let config = wallet_database::entities::config::Currency {
            currency: fiat.to_string(),
        };
        ConfigDomain::set_currency(Some(config)).await?;

        Ok(())
    }

    pub async fn set_app_id(mut self, app_id: &str) -> Result<(), crate::ServiceError> {
        let tx = &mut self.repo;
        tx.update_app_id(app_id).await?;
        let device = tx.get_device_info().await?;
        let wallet_list = tx.wallet_list().await?;
        for wallet in wallet_list {
            if let Some(device) = &device {
                let client_id = crate::domain::app::DeviceDomain::client_id_by_device(device)?;
                let keys_init_req = wallet_transport_backend::request::KeysInitReq::new(
                    &wallet.uid,
                    &device.sn,
                    Some(client_id),
                    device.app_id.clone(),
                    Some(device.device_type.clone()),
                    &wallet.name,
                );
                let keys_init_task_data = BackendApiTaskData::new(
                    wallet_transport_backend::consts::endpoint::KEYS_INIT,
                    &keys_init_req,
                )?;
                Tasks::new()
                    .push(Task::BackendApi(BackendApiTask::BackendApi(
                        keys_init_task_data,
                    )))
                    .send()
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn get_fiat(&self) -> Result<GetFiatRes, crate::ServiceError> {
        let config = crate::app_state::APP_STATE.read().await;

        Ok(GetFiatRes {
            fiat: config.currency().to_string(),
        })
    }

    pub async fn set_block_browser_url(&mut self) -> Result<(), crate::ServiceError> {
        // let tx = &mut self.repo;
        let backend_api = crate::manager::Context::get_global_backend_api()?;
        let block_browser_url_list = backend_api
            .chain_list()
            .await?
            .list
            .into_iter()
            .map(|info| {
                crate::request::init::BlockBrowserUrl::new(
                    info.chain_code,
                    info.address_url,
                    info.hash_url,
                )
            })
            .collect();
        let value = wallet_utils::serde_func::serde_to_string(&block_browser_url_list)?;
        ConfigDomain::set_config(BLOCK_BROWSER_URL_LIST, &value).await?;
        let mut config = crate::app_state::APP_STATE.write().await;
        config.set_block_browser_url(block_browser_url_list);
        Ok(())
    }

    pub async fn upload_log_file(self) -> Result<(), crate::ServiceError> {
        Ok(())
    }

    pub async fn mqtt_subscribe(
        &self,
        topics: Vec<String>,
        qos: Option<u8>,
    ) -> Result<(), crate::ServiceError> {
        // 获取全局 topics
        let global_topics = crate::manager::Context::get_global_mqtt_topics()?;
        let mut global_topics = global_topics.write().await;

        global_topics.subscribe(topics, qos).await?;

        Ok(())
    }

    pub async fn mqtt_unsubscribe_unsubscribe(
        &mut self,
        topics: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        // 获取全局已订阅的主题
        let global_topics = crate::manager::Context::get_global_mqtt_topics()?;
        let mut global_topics = global_topics.write().await;

        global_topics.unsubscribe(topics).await?;

        Ok(())
    }

    pub async fn mqtt_resubscribe(self) -> Result<(), crate::ServiceError> {
        // 获取全局已订阅的主题
        let global_topics = crate::manager::Context::get_global_mqtt_topics()?;
        let global_topics = global_topics.write().await;

        global_topics.resubscribe().await?;

        Ok(())
    }

    pub async fn get_configs(&self) -> Result<Vec<ConfigEntity>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let res = ConfigDao::lists(pool.as_ref()).await?;
        Ok(res)
    }

    // pub async fn set_config(
    //     &self,
    //     key: String,
    //     value: String,
    // ) -> Result<ConfigEntity, crate::ServiceError> {
    //     let pool = crate::manager::Context::get_global_sqlite_pool()?;

    // let min_config =
    //     wallet_database::entities::config::MinValueSwitchConfig::try_from(value.clone())?;

    // let res = ConfigDao::upsert(&key, &value, pool.as_ref()).await?;

    // // Report to the backend
    // let cx = crate::Context::get_context()?;

    // let sn = cx.device.sn.clone();
    // tracing::warn!("report sn = {}", sn);
    // let req = wallet_transport_backend::response_vo::app::SaveSendMsgAccount {
    //     sn: sn.clone(),
    //     amount: min_config.value,
    //     is_open: min_config.switch,
    // };

    // let backend = crate::Context::get_global_backend_api()?;
    // if let Err(e) = backend.save_send_msg_account(req).await {
    //     tracing::warn!("filter min value report faild sn = {} error = {}", sn, e);
    // }

    // Ok(res)
    // }

    pub async fn set_min_value_config(
        &self,
        symbol: String,
        amount: f64,
        switch: bool,
    ) -> Result<MinValueSwitchConfig, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let cx = crate::Context::get_context()?;
        let sn = cx.device.sn.clone();

        let symbol = symbol.to_ascii_uppercase();
        let key = MinValueSwitchConfig::get_key(&symbol, &sn);
        let config = MinValueSwitchConfig::new(switch, amount);

        ConfigDao::upsert(&key, &config.to_json_str()?, Some(1), pool.as_ref()).await?;

        let req = wallet_transport_backend::response_vo::app::SaveSendMsgAccount {
            sn: sn.clone(),
            amount,
            symbol,
            is_open: switch,
        };
        let backend = crate::Context::get_global_backend_api()?;
        if let Err(e) = backend.save_send_msg_account(vec![req]).await {
            tracing::warn!("filter min value report faild sn = {} error = {}", sn, e);
        }

        Ok(config)
    }

    pub async fn get_min_value_config(
        &self,
        symbol: String,
    ) -> Result<Option<MinValueSwitchConfig>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let symbol = symbol.to_uppercase();
        let cx = crate::Context::get_context()?;
        let sn = cx.device.sn.clone();

        let key = MinValueSwitchConfig::get_key(&symbol, &sn);

        match ConfigDao::find_by_key(&key, pool.as_ref()).await? {
            Some(r) => Ok(Some(MinValueSwitchConfig::try_from(r.value)?)),
            None => Ok(None),
        }
    }

    pub async fn app_install_save(
        self,
        sn: &str,
        device_type: &str,
        channel: &str,
    ) -> Result<(), crate::ServiceError> {
        let req = AppInstallSaveReq::new(sn, device_type, channel);
        let backend = crate::manager::Context::get_global_backend_api()?;
        backend.app_install_save(req).await?;
        Ok(())
    }
}
