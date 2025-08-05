pub mod token_price;
use super::app::config::ConfigDomain;
use crate::{
    infrastructure::parse_utc_datetime,
    response_vo::coin::{TokenCurrencies, TokenCurrencyId},
};
use chrono::{DateTime, Utc};
pub use token_price::TokenCurrencyGetter;
use wallet_database::{
    entities::coin::{CoinData, CoinEntity},
    repositories::{coin::CoinRepoTrait, exchange_rate::ExchangeRateRepoTrait, ResourcesRepo},
    DbPool,
};
use wallet_transport_backend::{response_vo::coin::TokenCurrency, CoinInfo};
use wallet_types::chain::chain::ChainCode;

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

    pub(crate) async fn upsert_hot_coin_list(
        repo: &mut ResourcesRepo,
        coins: Vec<CoinData>,
    ) -> Result<(), crate::ServiceError> {
        let mut seen = std::collections::HashSet::new();
        let mut coin_data = Vec::with_capacity(coins.len());

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

    pub async fn init_coins(repo: &mut ResourcesRepo) -> Result<(), crate::ServiceError> {
        let list: Vec<CoinData> = crate::default_data::coin::init_default_coins_list()?
            .coins
            .iter()
            .map(|coin| coin.to_owned().into())
            .collect();
        Self::upsert_hot_coin_list(repo, list).await?;

        Ok(())
    }

    pub fn get_stable_coin(chain_code: ChainCode) -> Result<String, crate::ServiceError> {
        match chain_code {
            ChainCode::Ethereum => Ok("0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string()),
            ChainCode::BnbSmartChain => {
                Ok("0x55d398326f99059fF775485246999027B3197955".to_string())
            }
            ChainCode::Tron => Ok("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string()),
            _ => Err(crate::BusinessError::Coin(crate::CoinError::NotFound(
                chain_code.to_string(),
            )))?,
        }
    }

    pub async fn fetch_all_coin(_pool: &DbPool) -> Result<Vec<CoinInfo>, crate::ServiceError> {
        // 本地没有币拉服务端所有的币,有拉去创建时间后的币种
        let backend_api = crate::Context::get_global_backend_api()?;
        let mut coins = Vec::new();

        // TODO 1.5 版本还是查询所有的币，避免版本更新后导致未更新的查询不到数据
        let create_at = None;

        // let create_at = if let Some(last_coin) = CoinRepo::last_coin(pool, true).await? {
        //     let formatted = last_coin.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
        //     Some(formatted)
        // } else {
        //     None
        // };

        coins.append(
            &mut backend_api
                .fetch_all_tokens(create_at.clone(), None)
                .await?,
        );

        // // 如果本地没有币，则添加默认币种并进行去重(感觉不是一个很好的逻辑)
        // if create_at.is_none() {
        //     let default = crate::default_data::coin::init_default_coins_list()?;

        //     let existing_keys: HashSet<_> = coins
        //         .iter()
        //         .map(|c| {
        //             (
        //                 c.chain_code.clone(),
        //                 c.symbol.clone(),
        //                 c.token_address.clone(),
        //             )
        //         })
        //         .collect();

        //     let mut default_list: Vec<wallet_transport_backend::CoinInfo> = default
        //         .coins
        //         .iter()
        //         .map(|coin| coin.to_owned().into())
        //         .filter(|default_coin: &wallet_transport_backend::CoinInfo| {
        //             !existing_keys.contains(&(
        //                 default_coin.chain_code.clone(),
        //                 default_coin.symbol.clone(),
        //                 default_coin.token_address.clone(),
        //             ))
        //         })
        //         .collect();

        //     // 添加缺失的默认币
        //     coins.append(&mut default_list);
        // }

        Ok(coins)
    }
}

impl From<crate::default_data::coin::DefaultCoin> for CoinData {
    fn from(coin: crate::default_data::coin::DefaultCoin) -> Self {
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
        created_at: parse_utc_datetime(&coin.create_time),
        updated_at: parse_utc_datetime(&coin.update_time),
    }
}

// pub(crate) struct UpsertCoinVo {
//     chain_code: Option<String>,
//     symbol: Option<String>,
//     name: Option<String>,
//     token_address: Option<String>,
//     decimals: Option<u8>,
//     protocol: Option<String>,
//     is_default: u8,
//     is_popular: u8,
//     status: u8,
// }

// impl From<wallet_transport_backend::CoinInfo> for CoinData {
//     fn from(coin: wallet_transport_backend::CoinInfo) -> Self {
//         Self {
//             chain_code: coin.chain_code.unwrap_or_default(),
//             symbol: coin.symbol.unwrap_or_default(),
//             name: coin.name,
//             token_address: coin.token_address,
//             decimals: coin.decimals.unwrap_or_default(),
//             protocol: coin.protocol,
//             is_default: if coin.default_token { 1 } else { 0 },
//             is_popular: if coin.popular_token { 1 } else { 0 },
//             status: if coin.popular_token { 1 } else { 0 },
//             is_custom: 0,
//             price: Some("0".to_string()),
//         }
//     }
// }

// impl UpsertCoinVo {
//     pub(crate) fn token_address(&self) -> Option<String> {
//         match &self.token_address {
//             Some(token_address) => {
//                 if token_address.is_empty() {
//                     None
//                 } else {
//                     Some(token_address.clone())
//                 }
//             }
//             None => None,
//         }
//     }
// }

// impl From<wallet_transport_backend::CoinInfo> for UpsertCoinVo {
//     fn from(coin: wallet_transport_backend::CoinInfo) -> Self {
//         Self {
//             chain_code: coin.chain_code,
//             symbol: coin.symbol,
//             name: coin.name,
//             token_address: coin.token_address,
//             decimals: coin.decimals,
//             protocol: coin.protocol,
//             is_default: if coin.default_token { 1 } else { 0 },
//             is_popular: if coin.popular_token { 1 } else { 0 },
//             // status: if coin.enable { 1 } else { 0 },
//             status: 1,
//         }
//     }
// }
