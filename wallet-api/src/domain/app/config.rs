use std::cmp::Ordering;
use wallet_crypto::KdfAlgorithm;
use wallet_database::{
    dao::config::ConfigDao,
    entities::config::{
        config_key::{
            APP_DOWNLOAD_QR_CODE_URL, APP_VERSION, BLOCK_BROWSER_URL_LIST, CURRENCY, INVITE_CODE,
            KEYSTORE_KDF_ALGORITHM, KEYS_RESET_STATUS, LANGUAGE, MQTT_URL, OFFICIAL_WEBSITE,
            WALLET_TREE_STRATEGY,
        },
        AppVersion, Currency, InviteCode, KeysResetStatus, MinValueSwitchConfig, MqttUrl,
        OfficialWebsite,
    },
};
use wallet_transport_backend::response_vo::chain::ChainUrlInfo;

pub struct ConfigDomain;

impl ConfigDomain {
    pub async fn get_config_min_value(symbol: &str) -> Result<Option<f64>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let cx = crate::Context::get_context()?;
        let sn = cx.device.sn.clone();
        let key = MinValueSwitchConfig::get_key(symbol, &sn);

        if let Some(config) = ConfigDao::find_by_key(&key, pool.as_ref()).await? {
            let min_config = MinValueSwitchConfig::try_from(config.value)?;
            if !min_config.switch {
                return Ok(None);
            }

            return Ok(Some(min_config.value));
        };

        Ok(None)
    }

    /// fetch the minimum filtering amount configuration to the backend each time a wallet is created.
    pub async fn fetch_min_config(sn: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;

        let backend = crate::Context::get_global_backend_api()?;
        let res = backend.fetch_min_config(sn.to_string()).await?;

        for item in res.list {
            let key = MinValueSwitchConfig::get_key(&item.token_code.to_uppercase(), sn);
            let value = MinValueSwitchConfig::new(item.is_open, item.min_amount);

            if let Err(e) =
                ConfigDao::upsert(&key, &value.to_json_str()?, Some(1), pool.as_ref()).await
            {
                tracing::error!("从后端同步过滤最小金额失败{}", e)
            }
        }

        Ok(())
    }

    pub async fn set_config(key: &str, value: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        ConfigDao::upsert(key, value, None, pool.as_ref()).await?;

        Ok(())
    }

    pub async fn set_official_website(website: Option<String>) -> Result<(), crate::ServiceError> {
        if let Some(official_website) = website {
            let config = OfficialWebsite {
                url: official_website.clone(),
            };
            ConfigDomain::set_config(OFFICIAL_WEBSITE, &config.to_json_str()?).await?;
            let mut config = crate::app_state::APP_STATE.write().await;
            config.set_official_website(Some(official_website));
        }

        Ok(())
    }

    pub async fn set_invite_code(
        status: Option<bool>,
        code: Option<String>,
    ) -> Result<(), crate::ServiceError> {
        let config = InviteCode { code, status };
        ConfigDomain::set_config(INVITE_CODE, &config.to_json_str()?).await?;

        Ok(())
    }

    pub async fn set_currency(currency: Option<Currency>) -> Result<(), crate::ServiceError> {
        let mut config = crate::app_state::APP_STATE.write().await;
        let currency = if let Some(currency) = currency
            && currency.currency != config.currency()
        {
            config.set_fiat_from_str(&currency.currency);
            currency
        } else {
            Currency::default()
        };
        drop(config);
        ConfigDomain::set_config(CURRENCY, &currency.to_json_str()?).await?;

        Ok(())
    }

    // pub async fn set_mqtt_url(mqtt_url: Option<String>) -> Result<(), crate::ServiceError> {
    //     if let Some(mqtt_url) = mqtt_url {
    //         let config = MqttUrl {
    //             url: mqtt_url.clone(),
    //         };

    //         tracing::info!("set mqtt url: {}", mqtt_url);
    //         ConfigDomain::set_config(MQTT_URL, &config.to_json_str()?).await?;
    //         crate::Context::set_global_mqtt_url(&mqtt_url).await?;
    //         let mut config = crate::app_state::APP_STATE.write().await;
    //         config.set_mqtt_url(Some(mqtt_url));
    //     }

    //     Ok(())
    // }

    pub async fn set_app_download_qr_code_url(
        app_download_qr_code_url: &str,
    ) -> Result<(), crate::ServiceError> {
        // let tx = &mut self.repo;
        let config = wallet_database::entities::config::AppInstallDownload {
            url: app_download_qr_code_url.to_string(),
        };
        ConfigDomain::set_config(APP_DOWNLOAD_QR_CODE_URL, &config.to_json_str()?).await?;
        let mut config = crate::app_state::APP_STATE.write().await;
        config.set_app_download_qr_code_url(Some(app_download_qr_code_url.to_string()));
        Ok(())
    }

    // pub async fn set_version_download_url(
    //     app_install_download_url: &str,
    // ) -> Result<(), crate::ServiceError> {
    //     // let tx = &mut self.repo;
    //     let encoded_url = urlencoding::encode(app_install_download_url);
    //     let url = format!("{}/{}/{}", BASE_URL, VERSION_DOWNLOAD, encoded_url);
    //     let config = wallet_database::entities::config::VersionDownloadUrl::new(&url);
    //     ConfigDomain::set_config(APP_DOWNLOAD_URL, &config.to_json_str()?).await?;
    //     let mut config = crate::app_state::APP_STATE.write().await;
    //     config.set_app_download_url(Some(url));
    //     Ok(())
    // }

    pub async fn init_app_install_download_url() -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let app_install_download_url =
            ConfigDao::find_by_key(APP_DOWNLOAD_QR_CODE_URL, pool.as_ref()).await?;
        if let Some(app_install_download_url) = app_install_download_url {
            let app_install_download_url =
                OfficialWebsite::try_from(app_install_download_url.value)?;

            let mut config = crate::app_state::APP_STATE.write().await;
            config.set_app_download_qr_code_url(Some(app_install_download_url.url));
        }
        Ok(())
    }

    // pub(crate) async fn init_mqtt_url() -> Result<(), crate::ServiceError> {
    //     let pool = crate::manager::Context::get_global_sqlite_pool()?;
    //     let config = ConfigDao::find_by_key(MQTT_URL, pool.as_ref()).await?;
    //     if let Some(config) = config {
    //         let mqtt_url = MqttUrl::try_from(config.value)?;
    //         crate::Context::set_global_mqtt_url(&mqtt_url.url).await?;
    //         let mut config = crate::app_state::APP_STATE.write().await;
    //         config.set_mqtt_url(Some(mqtt_url.url));
    //     }
    //     Ok(())
    // }

    pub async fn init_official_website() -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let official_website = ConfigDao::find_by_key(OFFICIAL_WEBSITE, pool.as_ref()).await?;
        if let Some(official_website) = official_website {
            let official_website = OfficialWebsite::try_from(official_website.value)?;

            let mut config = crate::app_state::APP_STATE.write().await;
            config.set_official_website(Some(official_website.url));
        }
        Ok(())
    }

    pub async fn init_currency() -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let currency = ConfigDao::find_by_key(CURRENCY, pool.as_ref()).await?;
        if let Some(currency) = currency {
            let mut config = crate::app_state::APP_STATE.write().await;
            let currency = wallet_database::entities::config::Currency::try_from(currency.value)?;
            config.set_fiat_from_str(&currency.currency);
        } else {
            ConfigDomain::set_currency(None).await?;
        };
        Ok(())
    }

    pub async fn init_language() -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let language = ConfigDao::find_by_key(LANGUAGE, pool.as_ref()).await?;
        let mut config = crate::app_state::APP_STATE.write().await;
        if let Some(language) = language {
            let language = wallet_database::entities::config::Language::try_from(language.value)?;
            config.set_language(&language.language);
        } else {
            let l = config.language();
            let config = wallet_database::entities::config::Language::new(l);
            ConfigDomain::set_config(LANGUAGE, &config.to_json_str()?).await?;
        };
        drop(config);

        Ok(())
    }

    pub(crate) async fn get_currency() -> Result<String, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let currency = ConfigDao::find_by_key(CURRENCY, pool.as_ref()).await?;
        if let Some(currency) = currency {
            let currency = wallet_database::entities::config::Currency::try_from(currency.value)?;
            Ok(currency.currency)
        } else {
            Ok(String::from("USD"))
        }
    }

    pub(crate) async fn get_invite_code() -> Result<Option<InviteCode>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let invite_code = ConfigDao::find_by_key(INVITE_CODE, pool.as_ref()).await?;

        invite_code
            .map(|invite_code| {
                let invite_code =
                    wallet_database::entities::config::InviteCode::try_from(invite_code.value)?;
                Ok(invite_code)
            })
            .transpose()

        // if let Some(invite_code) = invite_code {
        //     let invite_code =
        //         wallet_database::entities::config::InviteCode::try_from(invite_code.value)?;
        //     Ok(invite_code)
        // } else {
        //     Err(crate::BusinessError::Device(crate::DeviceError::InviteStatusNotConfirmed).into())
        // }
    }

    pub async fn set_keys_reset_status(status: Option<bool>) -> Result<(), crate::ServiceError> {
        let config = KeysResetStatus { status };
        ConfigDomain::set_config(KEYS_RESET_STATUS, &config.to_json_str()?).await?;

        Ok(())
    }

    pub(crate) async fn get_keys_reset_status(
    ) -> Result<Option<KeysResetStatus>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let keys_reset_status = ConfigDao::find_by_key(KEYS_RESET_STATUS, pool.as_ref()).await?;

        if let Some(keys_reset_status) = keys_reset_status {
            Ok(Some(KeysResetStatus::try_from(keys_reset_status.value)?))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn get_app_version() -> Result<AppVersion, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let app_version = ConfigDao::find_by_key(APP_VERSION, pool.as_ref())
            .await?
            .ok_or(crate::ServiceError::Business(crate::BusinessError::Config(
                crate::ConfigError::NotFound(APP_VERSION.to_owned()),
            )))?;
        Ok(AppVersion::try_from(app_version.value)?)
    }

    pub(crate) fn compare_versions(v1: &str, v2: &str) -> Ordering {
        let parse = |v: &str| {
            v.split('.')
                .map(|s| s.parse::<u32>().unwrap_or(0))
                .collect::<Vec<_>>()
        };

        let mut v1_parts = parse(v1);
        let mut v2_parts = parse(v2);

        let max_len = v1_parts.len().max(v2_parts.len());
        v1_parts.resize(max_len, 0);
        v2_parts.resize(max_len, 0);

        for (a, b) in v1_parts.iter().zip(v2_parts.iter()) {
            match a.cmp(b) {
                Ordering::Equal => continue,
                non_eq => return non_eq,
            }
        }

        Ordering::Equal
    }

    pub(crate) async fn get_keystore_kdf_algorithm() -> Result<KdfAlgorithm, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let keystore_kdf_algorithm =
            ConfigDao::find_by_key(KEYSTORE_KDF_ALGORITHM, pool.as_ref()).await?;
        if let Some(keystore_kdf_algorithm) = keystore_kdf_algorithm {
            let keystore_kdf_algorithm =
                wallet_database::entities::config::KeystoreKdfAlgorithm::try_from(
                    keystore_kdf_algorithm.value,
                )?;
            Ok(keystore_kdf_algorithm.keystore_kdf_algorithm)
        } else {
            Ok(KdfAlgorithm::Scrypt)
            // Ok(KdfAlgorithm::Argon2id)
        }
    }

    pub(crate) async fn get_wallet_tree_strategy(
    ) -> Result<wallet_tree::WalletTreeStrategy, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let wallet_tree_strategy =
            ConfigDao::find_by_key(WALLET_TREE_STRATEGY, pool.as_ref()).await?;
        if let Some(wallet_tree_strategy) = wallet_tree_strategy {
            let wallet_tree_strategy =
                wallet_database::entities::config::WalletTreeStrategy::try_from(
                    wallet_tree_strategy.value,
                )?;
            Ok(wallet_tree_strategy.wallet_tree_strategy)
        } else {
            // Ok(wallet_tree::WalletTreeStrategy::V2)
            Ok(wallet_tree::WalletTreeStrategy::V1)
        }
    }

    pub async fn init_block_browser_url_list() -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let block_browser_url_list =
            ConfigDao::find_by_key(BLOCK_BROWSER_URL_LIST, pool.as_ref()).await?;
        if let Some(block_browser_url_list) = block_browser_url_list {
            let mut config = crate::app_state::APP_STATE.write().await;
            let value = wallet_utils::serde_func::serde_from_str(&block_browser_url_list.value)?;

            config.set_block_browser_url(value);
        }

        Ok(())
    }

    pub(crate) async fn set_block_browser_url(
        list: &[ChainUrlInfo],
    ) -> Result<(), crate::ServiceError> {
        let block_browser_url_list = list
            .iter()
            .map(|info| {
                crate::request::init::BlockBrowserUrl::new(
                    info.chain_code.clone(),
                    info.address_url.clone(),
                    info.hash_url.clone(),
                )
            })
            .collect();
        let value = wallet_utils::serde_func::serde_to_string(&block_browser_url_list)?;
        ConfigDomain::set_config(BLOCK_BROWSER_URL_LIST, &value).await?;
        let mut config = crate::app_state::APP_STATE.write().await;
        config.set_block_browser_url(block_browser_url_list);
        Ok(())
    }

    pub async fn init_url() -> Result<(), crate::ServiceError> {
        // Self::init_mqtt_url().await?;
        // crate::WalletManager::init_mqtt().await?;

        Self::init_official_website().await?;
        Self::init_block_browser_url_list().await?;
        Self::init_app_install_download_url().await?;
        Self::init_language().await?;

        Ok(())
    }

    // Attempt to get the MQTT URI from the backend.
    // If an error occurs or the URI is not found, use the URI from the database instead.
    pub async fn get_mqtt_uri() -> Result<Option<String>, crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;

        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        if let Ok(mqtt_url) = backend_api.mqtt_init().await {
            let config = MqttUrl {
                url: mqtt_url.clone(),
            };
            ConfigDomain::set_config(MQTT_URL, &config.to_json_str()?).await?;
            return Ok(Some(config.url_with_protocol()));
        }

        let config = ConfigDao::find_by_key(MQTT_URL, pool.as_ref()).await?;
        let uri = config
            .and_then(|c| MqttUrl::try_from(c.value).ok())
            .map(|mqtt| mqtt.url_with_protocol());

        Ok(uri)
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use crate::domain::app::config::ConfigDomain;

    #[test]
    fn test_equal_versions() {
        assert_eq!(
            ConfigDomain::compare_versions("1.2.3", "1.2.3"),
            Ordering::Equal
        );
        assert_eq!(
            ConfigDomain::compare_versions("1.2", "1.2.0"),
            Ordering::Equal
        );
        assert_eq!(
            ConfigDomain::compare_versions("1.0.0.0", "1"),
            Ordering::Equal
        );
    }

    #[test]
    fn test_greater_versions() {
        assert_eq!(
            ConfigDomain::compare_versions("1.2.10", "1.2.2"),
            Ordering::Greater
        );
        assert_eq!(
            ConfigDomain::compare_versions("2.0", "1.999.999"),
            Ordering::Greater
        );
        assert_eq!(
            ConfigDomain::compare_versions("1.10.0", "1.2.99"),
            Ordering::Greater
        );
    }

    #[test]
    fn test_less_versions() {
        assert_eq!(
            ConfigDomain::compare_versions("0.9.9", "1.0.0"),
            Ordering::Less
        );
        assert_eq!(
            ConfigDomain::compare_versions("1.2.3", "1.2.4"),
            Ordering::Less
        );
        assert_eq!(
            ConfigDomain::compare_versions("1.2", "1.2.1"),
            Ordering::Less
        );
    }

    #[test]
    fn test_invalid_parts() {
        assert_eq!(
            ConfigDomain::compare_versions("1.2.alpha", "1.2.0"),
            Ordering::Equal
        ); // "alpha" -> 0
        assert_eq!(
            ConfigDomain::compare_versions("1.a.3", "1.0.3"),
            Ordering::Equal
        );
        assert_eq!(
            ConfigDomain::compare_versions("a.b.c", "0.0.0"),
            Ordering::Equal
        );
    }

    #[test]
    fn test_empty_strings() {
        assert_eq!(ConfigDomain::compare_versions("", ""), Ordering::Equal);
        assert_eq!(
            ConfigDomain::compare_versions("1.2.3", ""),
            Ordering::Greater
        );
        assert_eq!(ConfigDomain::compare_versions("", "0.0.1"), Ordering::Less);
    }

    #[test]
    fn test_trailing_zeros() {
        assert_eq!(
            ConfigDomain::compare_versions("1.0.0.0", "1"),
            Ordering::Equal
        );
        assert_eq!(
            ConfigDomain::compare_versions("1.0.0.1", "1"),
            Ordering::Greater
        );
        assert_eq!(
            ConfigDomain::compare_versions("1", "1.0.0.1"),
            Ordering::Less
        );
    }

    #[test]
    fn test_long_versions() {
        assert_eq!(
            ConfigDomain::compare_versions("1.2.3.4.5.6.7", "1.2.3.4.5.6.7"),
            Ordering::Equal
        );
        assert_eq!(
            ConfigDomain::compare_versions("1.2.3.4.5.6.8", "1.2.3.4.5.6.7"),
            Ordering::Greater
        );
    }
}
