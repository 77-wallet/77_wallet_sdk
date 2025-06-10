pub mod token_price;
use crate::response_vo::coin::{TokenCurrencies, TokenCurrencyId, TokenPriceChangeRes};
pub use token_price::TokenCurrencyGetter;
use wallet_database::{
    entities::coin::{CoinData, CoinEntity, CoinId},
    repositories::{coin::CoinRepoTrait, exchange_rate::ExchangeRateRepoTrait, ResourcesRepo},
};
use wallet_transport_backend::{request::TokenQueryPriceReq, response_vo::coin::TokenCurrency};

use super::app::config::ConfigDomain;

pub struct CoinDomain {}
impl Default for CoinDomain {
    fn default() -> Self {
        Self::new()
    }
}
impl CoinDomain {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn get_coin(
        chain_code: &str,
        symbol: &str,
    ) -> Result<CoinEntity, crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;

        let coin = CoinEntity::get_coin(chain_code, symbol, pool.as_ref())
            .await?
            .ok_or(crate::BusinessError::Coin(crate::CoinError::NotFound(
                symbol.to_string(),
            )))?;

        Ok(coin)
    }

    /// 查询代币汇率
    pub async fn get_token_currencies_v2(
        &mut self,
        repo: &mut ResourcesRepo,
    ) -> Result<TokenCurrencies, crate::ServiceError> {
        // let config = crate::app_state::APP_STATE.read().await;
        // let currency = config.currency();
        // let currency = "USD";
        let currency = ConfigDomain::get_currency().await?;

        let coins = repo.coin_list(None, None).await?;

        let exchange_rate_list = repo.list().await?;
        // 查询本地的所有币符号
        let mut map = std::collections::HashMap::new();
        for coin in coins {
            let price = coin.price.parse::<f64>().unwrap_or_default();
            let (currency_price, rate) = if let Some(rate) = exchange_rate_list
                .iter()
                .find(|rate| rate.target_currency == currency)
            {
                (price * rate.rate, rate.rate)
            } else {
                (f64::default(), f64::default())
            };

            let symbol = coin.symbol.to_ascii_lowercase();
            let name = coin.name;

            let token_currency_id = TokenCurrencyId::new(&symbol, &coin.chain_code);

            let token_currency = TokenCurrency {
                name,
                chain_code: coin.chain_code,
                code: symbol.clone(),
                price: Some(price),
                currency_price: Some(currency_price),
                rate,
            };
            map.insert(token_currency_id, token_currency);
        }

        Ok(TokenCurrencies(map))
    }

    pub async fn get_token_price(
        &mut self,
        repo: &mut ResourcesRepo,
        symbols: Vec<String>,
    ) -> Result<Vec<TokenPriceChangeRes>, crate::ServiceError> {
        let tx = repo;
        let backend_api = crate::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let coins = tx.coin_list_with_symbols(&symbols, None).await?;

        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());

        coins.into_iter().for_each(|coin| {
            let contract_address = coin.token_address.clone().unwrap_or_default();
            req.insert(&coin.chain_code, &contract_address);
        });

        let tokens = backend_api.token_query_price(cryptor, req).await?.list;

        let config = crate::app_state::APP_STATE.read().await;
        let currency = config.currency();

        let exchange_rate = ExchangeRateRepoTrait::detail(tx, Some(currency.to_string())).await?;

        let mut res = Vec::new();
        if let Some(exchange_rate) = exchange_rate {
            for mut token in tokens {
                if let Some(symbol) = symbols
                    .iter()
                    .find(|s| s.to_lowercase() == token.symbol.to_lowercase())
                {
                    token.symbol = symbol.to_string();
                    let coin_id = CoinId {
                        chain_code: token.chain_code.clone(),
                        symbol: symbol.to_string(),
                        token_address: token.token_address.clone(),
                    };
                    tx.update_price_unit(&coin_id, &token.price.to_string(), token.unit)
                        .await?;
                    let data =
                        TokenCurrencies::calculate_token_price_changes(token, exchange_rate.rate)
                            .await?;
                    res.push(data);
                }
            }
        }

        Ok(res)
    }

    pub(crate) async fn upsert_hot_coin_list(
        repo: &mut ResourcesRepo,
        coins: Vec<UpsertCoinVo>,
    ) -> Result<(), crate::ServiceError> {
        let tx = repo;
        let mut coin_datas = Vec::new();
        for coin in coins {
            let Some(symbol) = &coin.symbol else { continue };
            let Some(chain_code) = &coin.chain_code else {
                continue;
            };
            let token_address = coin.token_address();

            // 检查该币种是否已在 coin_datas 中存在
            if coin_datas.iter().any(|c: &CoinData| {
                c.symbol == *symbol
                    && c.chain_code == *chain_code
                    && c.token_address() == token_address
            }) {
                continue;
            }

            // 如果不存在，新增 CoinData
            coin_datas.push(
                CoinData::new(
                    coin.name.clone(),
                    symbol,
                    chain_code,
                    token_address,
                    None,
                    coin.protocol.clone(),
                    coin.decimals.unwrap_or_default(),
                    coin.is_default,
                    coin.is_popular,
                    coin.status,
                )
                .with_status(coin.status),
            );
        }

        tx.upsert_multi_coin(coin_datas).await?;
        Ok(())
    }

    pub async fn init_coins(repo: &mut ResourcesRepo) -> Result<(), crate::ServiceError> {
        // TODO: 下版本后，需要将该方法移除
        // repo.clean_table().await?;
        let list: Vec<UpsertCoinVo> = crate::default_data::coin::init_default_coins_list()?
            .coins
            .iter()
            .map(|coin| coin.to_owned().into())
            .collect();
        Self::upsert_hot_coin_list(repo, list).await?;

        Ok(())
    }
}

impl From<crate::default_data::coin::DefaultCoin> for UpsertCoinVo {
    fn from(coin: crate::default_data::coin::DefaultCoin) -> Self {
        Self {
            chain_code: Some(coin.chain_code),
            symbol: Some(coin.symbol),
            name: Some(coin.name),
            token_address: coin.token_address,
            decimals: Some(coin.decimals),
            protocol: coin.protocol,
            is_default: if coin.default { 1 } else { 0 },
            is_popular: if coin.popular { 1 } else { 0 },
            // status: if coin.enable { 1 } else { 0 },
            status: 1,
        }
    }
}

pub(crate) struct UpsertCoinVo {
    chain_code: Option<String>,
    symbol: Option<String>,
    name: Option<String>,
    token_address: Option<String>,
    decimals: Option<u8>,
    protocol: Option<String>,
    is_default: u8,
    is_popular: u8,
    status: u8,
}

impl UpsertCoinVo {
    pub(crate) fn token_address(&self) -> Option<String> {
        match &self.token_address {
            Some(token_address) => {
                if token_address.is_empty() {
                    None
                } else {
                    Some(token_address.clone())
                }
            }
            None => None,
        }
    }
}

impl From<wallet_transport_backend::CoinInfo> for UpsertCoinVo {
    fn from(coin: wallet_transport_backend::CoinInfo) -> Self {
        Self {
            chain_code: coin.chain_code,
            symbol: coin.symbol,
            name: coin.name,
            token_address: coin.token_address,
            decimals: coin.decimals,
            protocol: coin.protocol,
            is_default: if coin.default_token { 1 } else { 0 },
            is_popular: if coin.popular_token { 1 } else { 0 },
            // status: if coin.enable { 1 } else { 0 },
            status: 1,
        }
    }
}
