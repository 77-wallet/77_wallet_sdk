pub mod token_price;
use std::collections::HashMap;

use super::app::config::ConfigDomain;
use crate::{
    infrastructure::parse_utc_datetime,
    response_vo::{
        chain::ChainList,
        coin::{CoinInfoList, TokenCurrencies, TokenCurrencyId},
    },
};
use chrono::{DateTime, Utc};
pub use token_price::TokenCurrencyGetter;
use wallet_database::{
    DbPool,
    entities::coin::{CoinData, CoinEntity},
    repositories::{
        ResourcesRepo,
        coin::{CoinRepo, CoinRepoTrait},
        exchange_rate::{ExchangeRateRepo, ExchangeRateRepoTrait},
    },
};
use wallet_transport_backend::{CoinInfo, response_vo::coin::TokenCurrency};
use wallet_types::chain::chain::ChainCode;

mod chain_stable_coin {
    pub const ETHEREUM: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
    pub const BNB_SMART_CHAIN: &str = "0x55d398326f99059fF775485246999027B3197955";
    pub const TRON: &str = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";
    pub const SOLANA: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";
}

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
        token_address: Option<String>,
    ) -> Result<CoinEntity, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let coin = CoinRepo::coin_by_symbol_chain(chain_code, symbol, token_address, &pool).await?;

        Ok(coin)
    }

    /// 查询代币汇率
    pub async fn get_token_currencies_v2()
    -> Result<TokenCurrencies, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let currency = ConfigDomain::get_currency().await?;

        let coins = CoinRepo::coin_list_v2(pool.clone(), None, None).await?;

        let exchange_rate_list = ExchangeRateRepo::list(&pool).await?;
        // 查询本地的所有币符号
        let mut map = std::collections::HashMap::new();
        for coin in coins {
            let price = coin.price.parse::<f64>().unwrap_or_default();
            let (currency_price, rate) = if let Some(rate) =
                exchange_rate_list.iter().find(|rate| rate.target_currency == currency)
            {
                (price * rate.rate, rate.rate)
            } else {
                (f64::default(), f64::default())
            };

            let symbol = &coin.symbol;
            let chain_code = &coin.chain_code;

            let token_currency_id = TokenCurrencyId::new(
                &symbol.to_ascii_lowercase(),
                chain_code,
                coin.token_address(),
            );

            let token_currency = TokenCurrency {
                name: coin.name,
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

    pub(crate) fn merge_coin_to_list(
        coins: Vec<CoinEntity>,
        show_contract: bool,
    ) -> Result<CoinInfoList, crate::error::service::ServiceError> {
        let mut data = CoinInfoList::default();

        for coin in coins.into_iter() {
            if let Some(d) = data
                .iter_mut()
                .find(|info| info.symbol == coin.symbol && info.is_default && coin.is_default == 1)
            {
                d.chain_list
                    .entry(coin.chain_code.clone())
                    .or_insert(coin.token_address.unwrap_or_default());
            } else {
                data.push(crate::response_vo::coin::CoinInfo {
                    symbol: coin.symbol.clone(),
                    name: Some(coin.name.clone()),
                    chain_list: ChainList(HashMap::from([(
                        coin.chain_code.clone(),
                        coin.token_address.unwrap_or_default(),
                    )])),
                    is_default: coin.is_default == 1,
                    hot_coin: coin.status == 1,
                    show_contract,
                })
            }
        }
        Ok(data)
    }

    pub(crate) async fn upsert_hot_coin_list(
        repo: &mut ResourcesRepo,
        coins: Vec<CoinData>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut seen = std::collections::HashSet::new();
        let mut coin_data = Vec::with_capacity(coins.len());

        // filter repeat
        for coin in coins {
            let key = (
                coin.symbol.clone(),
                coin.chain_code.clone(),
                coin.token_address.clone().unwrap_or_default(),
            );

            if seen.insert(key) {
                coin_data.push(coin);
            }
        }

        repo.upsert_multi_coin(coin_data).await?;
        Ok(())
    }

    pub async fn init_coins(
        repo: &mut ResourcesRepo,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = repo.pool();
        // check 本地表是否有数据,有则不进行新增
        let count = CoinRepo::coin_count(&pool).await?;
        if count > 0 {
            return Ok(());
        }

        let list: Vec<CoinData> = crate::default_data::coin::init_default_coins_list()?
            .coins
            .iter()
            .map(|coin| coin.to_owned().into())
            .collect();
        Self::upsert_hot_coin_list(repo, list).await?;

        Ok(())
    }

    // 每个链的主流 usdt代币合约地址
    pub fn get_stable_coin(
        chain_code: ChainCode,
    ) -> Result<&'static str, crate::error::service::ServiceError> {
        match chain_code {
            ChainCode::Ethereum => Ok(chain_stable_coin::ETHEREUM),
            ChainCode::BnbSmartChain => Ok(chain_stable_coin::BNB_SMART_CHAIN),
            ChainCode::Tron => Ok(chain_stable_coin::TRON),
            ChainCode::Solana => Ok(chain_stable_coin::SOLANA),
            _ => Err(crate::error::business::BusinessError::Coin(
                crate::error::business::coin::CoinError::NotFound(chain_code.to_string()),
            ))?,
        }
    }

    pub async fn fetch_all_coin(
        pool: &DbPool,
    ) -> Result<Vec<CoinInfo>, crate::error::service::ServiceError> {
        // 本地没有币拉服务端所有的币,有拉去创建时间后的币种
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut coins = Vec::new();

        // TODO 1.5 版本验证币数量如果大于500说明已经同步过最新的币了,拉最新的。
        // let create_at = None;
        let count = CoinRepo::coin_count(pool).await?;
        let create_at = if count > 500 {
            if let Some(last_coin) = CoinRepo::last_coin(pool, true).await? {
                let formatted = last_coin.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
                Some(formatted)
            } else {
                None
            }
        } else {
            None
        };

        coins.append(&mut backend_api.fetch_all_tokens(create_at.clone(), None).await?);

        Ok(coins)
    }
}

impl From<crate::default_data::coin::DefaultCoin> for CoinData {
    fn from(coin: crate::default_data::coin::DefaultCoin) -> Self {
        // 默认的代币:默认值支持兑换的
        Self {
            name: Some(coin.name),
            chain_code: coin.chain_code,
            symbol: coin.symbol,
            token_address: coin.token_address,
            decimals: coin.decimals,
            protocol: coin.protocol,
            is_default: if coin.default { 1 } else { 0 },
            is_popular: if coin.popular { 1 } else { 0 },
            is_custom: 0,
            price: Some("0".to_string()),
            status: if coin.active { 1 } else { 0 },
            swappable: true,
            created_at: DateTime::<Utc>::default(),
            updated_at: DateTime::<Utc>::default(),
        }
    }
}

pub fn coin_info_to_coin_data(coin: CoinInfo) -> CoinData {
    CoinData {
        chain_code: coin.chain_code.unwrap_or_default(),
        symbol: coin.symbol.unwrap_or_default(),
        name: coin.name,
        token_address: coin.token_address,
        decimals: coin.decimals.unwrap_or_default(),
        protocol: coin.protocol,
        is_default: if coin.default_token { 1 } else { 0 },
        is_popular: if coin.popular_token { 1 } else { 0 },
        is_custom: 0,
        price: Some(coin.price.unwrap_or_default().to_string()),
        status: if coin.enable { 1 } else { 0 },
        swappable: coin.swappable,
        created_at: parse_utc_datetime(&coin.create_time),
        updated_at: parse_utc_datetime(&coin.update_time),
    }
}
