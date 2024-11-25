use wallet_database::{
    dao::config::ConfigDao,
    entities::config::{config_key::MIN_VALUE_SWITCH, MinValueSwitchConfig},
};

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
}
