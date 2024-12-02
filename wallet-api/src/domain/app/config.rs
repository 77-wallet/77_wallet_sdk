use wallet_database::{
    dao::config::ConfigDao,
    entities::config::{
        config_key::{BLOCK_BROWSER_URL_LIST, MIN_VALUE_SWITCH, OFFICIAL_WEBSITE},
        MinValueSwitchConfig,
    },
};
use wallet_transport_backend::response_vo::app::SaveSendMsgAccount;

pub struct ConfigDomain;

impl ConfigDomain {
    // 获取配置过滤的最小币
    //    U的价值  = 配置的法币值  / 汇率
    //    币的数量  = U的价值     / 币的单价(币)
    pub async fn get_config_min_value(
        chain_code: &str,
        symbol: &str,
    ) -> Result<Option<f64>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        if let Some(config) = ConfigDao::find_by_key(MIN_VALUE_SWITCH, pool.as_ref()).await? {
            let min_config = MinValueSwitchConfig::try_from(config.value)?;
            if !min_config.switch {
                return Ok(None);
            }
            // 币价格
            let token_currency = super::super::coin::TokenCurrencyGetter::get_currency(
                &min_config.currency,
                chain_code,
                symbol,
            )
            .await?;

            if let Some(price) = token_currency.price {
                if token_currency.rate == 0.0 || price == 0.0 {
                    return Ok(None);
                }

                return Ok(Some(min_config.value / token_currency.rate / price));
            } else {
                return Ok(Some(0.0));
            }
        };

        Ok(None)
    }

    /// Report the minimum filtering amount configuration to the backend each time a wallet is created.
    pub async fn report_backend(sn: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;

        let config = ConfigDao::find_by_key(
            wallet_database::entities::config::config_key::MIN_VALUE_SWITCH,
            pool.as_ref(),
        )
        .await?;

        let req = if let Some(config) = config {
            let min_config =
                wallet_database::entities::config::MinValueSwitchConfig::try_from(config.value)?;
            SaveSendMsgAccount {
                amount: min_config.value,
                sn: sn.to_string(),
                is_open: min_config.switch,
            }
        } else {
            SaveSendMsgAccount {
                amount: 0.0,
                sn: sn.to_string(),
                is_open: false,
            }
        };

        let backend = crate::Context::get_global_backend_api()?;
        if let Err(e) = backend.save_send_msg_account(req).await {
            tracing::error!(sn = sn, "report min value error:{} ", e);
        }
        Ok(())
    }

    pub async fn set_config(key: &str, value: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        ConfigDao::upsert(key, value, pool.as_ref()).await?;

        Ok(())
    }

    pub async fn init_official_website() -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let official_website = ConfigDao::find_by_key(OFFICIAL_WEBSITE, pool.as_ref()).await?;
        if let Some(official_website) = official_website {
            Self::set_config(OFFICIAL_WEBSITE, &official_website.value).await?;
            let mut config = crate::config::CONFIG.write().await;
            config.set_official_website(Some(official_website.value));
        }
        Ok(())
    }

    pub async fn init_block_browser_url_list() -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let block_browser_url_list =
            ConfigDao::find_by_key(BLOCK_BROWSER_URL_LIST, pool.as_ref()).await?;
        if let Some(block_browser_url_list) = block_browser_url_list {
            Self::set_config(BLOCK_BROWSER_URL_LIST, &block_browser_url_list.value).await?;
            let mut config = crate::config::CONFIG.write().await;
            let value = wallet_utils::serde_func::serde_from_str(&block_browser_url_list.value)?;

            config.set_block_browser_url(value);
        }

        Ok(())
    }

    pub async fn init_url() -> Result<(), crate::ServiceError> {
        Self::init_official_website().await?;
        Self::init_block_browser_url_list().await?;

        Ok(())
    }
}
