use crate::response_vo::account::{BalanceInfo, BalanceStr};
use wallet_database::repositories::{coin::CoinRepo, exchange_rate::ExchangeRateRepo};
use wallet_transport_backend::response_vo::coin::TokenCurrency;
use wallet_utils::unit;

pub struct TokenCurrencyGetter;

impl TokenCurrencyGetter {
    // 从数据库里面获取对应的值
    // currency 法币符号,chain_code 链码  symbol 币符号
    pub async fn get_currency(
        currency: &str,
        chain_code: &str,
        symbol: &str,
    ) -> Result<TokenCurrency, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let coin = CoinRepo::coin_by_symbol_chain(chain_code, symbol, &pool).await?;
        // get rate
        let exchange = ExchangeRateRepo::exchange_rate(currency, &pool).await?;

        let (price, currency_price) = if !coin.price.is_empty() {
            let price = unit::string_to_f64(&coin.price)?;
            (Some(price), Some(exchange.rate * price))
        } else {
            (None, None)
        };

        Ok(TokenCurrency {
            chain_code: chain_code.to_string(),
            code: symbol.to_string(),
            name: coin.name,
            price,
            currency_price,
            rate: exchange.rate,
        })
    }

    pub async fn get_balance_info(
        chain_code: &str,
        symbol: &str,
        amount: f64,
    ) -> Result<BalanceInfo, crate::ServiceError> {
        let currency = {
            let state = crate::app_state::APP_STATE.read().await;
            state.currency().to_string() // 或复制 enum 值，取决于类型
        };

        let token_price = TokenCurrencyGetter::get_currency(&currency, chain_code, symbol).await?;

        Ok(BalanceInfo::new(
            amount,
            token_price.get_price(&currency),
            &currency,
        ))
    }

    // 查询后端的币价，并转换为balance数据结构
    pub async fn get_bal_by_backend(
        chain_code: &str,
        token_addr: &str,
        amount: &str,
        decimals: u8,
    ) -> Result<BalanceStr, crate::ServiceError> {
        let currency = {
            let state = crate::app_state::APP_STATE.read().await;
            state.currency().to_string() // 或复制 enum 值，取决于类型
        };

        let backend = crate::manager::Context::get_global_backend_api()?;
        let token_price = backend.token_price(chain_code, token_addr).await?;

        let price = unit::string_to_f64(&token_price.price)?;

        // 对应汇率的价格
        let unit_price = if currency.eq_ignore_ascii_case("usdt") {
            price
        } else {
            let pool = crate::manager::Context::get_global_sqlite_pool()?;
            let exchange = ExchangeRateRepo::exchange_rate(&currency, &pool).await?;

            exchange.rate * price
        };
        let amount = wallet_utils::unit::convert_to_u256(amount, decimals)?;

        Ok(BalanceStr::new(
            amount,
            Some(unit_price),
            &currency,
            decimals,
        )?)
    }
}
