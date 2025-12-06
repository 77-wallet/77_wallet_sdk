use std::collections::HashMap;

use chrono::{DateTime, Utc};
use wallet_database::{
    entities::{
        api_assets::ApiCreateAssetsVo,
        api_coin::{ApiCoinData, ApiCoinEntity},
        assets::AssetsId,
    },
    repositories::{
        api_wallet::{account::ApiAccountRepo, assets::ApiAssetsRepo, coin::ApiCoinRepo},
        exchange_rate::ExchangeRateRepo,
    },
};
use wallet_transport_backend::response_vo::{api_wallet::coin::ApiCoinInfo, coin::TokenCurrency};

use crate::{
    domain::app::config::ConfigDomain,
    infrastructure::task_queue::{initialization::InitializationTask, task::Tasks},
    response_vo::standard_wallet::{
        chain::ChainList,
        coin::{CoinInfoList, TokenCurrencies, TokenCurrencyId},
    },
};

impl From<crate::default_data::coin::DefaultCoin> for ApiCoinData {
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
            price: None,
            status: if coin.active { 1 } else { 0 },
            created_at: DateTime::<Utc>::default(),
            updated_at: Some(DateTime::<Utc>::default()),
        }
    }
}
pub struct ApiCoinDomain {}

impl ApiCoinDomain {
    pub async fn init_sync_api_coins() -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let coin_list = ApiCoinRepo::coin_list(&pool).await?;
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        for coin in &coin_list {
            let price: f64 = coin.price.parse().unwrap_or_default();
            if price != 0f64 {
                continue;
            }
            if let Some(token) = &coin.token_address {
                let coin_find = backend_api.token_price(&coin.chain_code, token).await.ok();
                if let Some(coin) = coin_find {
                    ApiCoinRepo::update_price_unit1(
                        &coin.code,
                        &coin.token_address,
                        &price.to_string(),
                        &pool,
                    )
                    .await?;
                }
            }
        }

        Ok(())
    }
    pub async fn init_api_coins() -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // check 本地表是否有数据,有则不进行新增
        // let count = ApiCoinRepo::coin_count(&pool).await?;
        // if count <= 0 {
        //     let list: Vec<ApiCoinData> = crate::default_data::coin::init_default_coins_list()?
        //         .coins
        //         .iter()
        //         .map(|coin| coin.to_owned().into())
        //         .collect();
        //     Self::upsert_hot_coin_list(list).await?;
        // }

        // 使用本地数据进行保底初始化，确保即使后端接口失败，系统也有数据可用
        let list = ApiCoinRepo::coin_list(&pool).await?;
        if !list.is_empty() {
            let mut coins_to_initialize = Vec::with_capacity(list.len());
            for coin in list {
                if let Ok(price_real) = wallet_utils::unit::string_to_f64(&coin.price) {
                    coins_to_initialize.push(
                        crate::infrastructure::asset_calc::actor_model::CoinInitializationData {
                            symbol: coin.symbol.clone(),
                            chain_code: coin.chain_code.clone(),
                            name: coin.name.clone(),
                            token_address: coin.token_address.clone(),
                            price_real,
                            decimals: coin.decimals,
                        },
                    );
                }
            }

            // 批量初始化币价
            if !coins_to_initialize.is_empty() {
                let asset_calc_actor_manager = crate::context::CONTEXT
                    .get()
                    .unwrap()
                    .get_global_asset_calc_actor_manager()
                    .await?;
                if let Err(e) =
                    asset_calc_actor_manager.batch_initialize_prices(coins_to_initialize).await
                {
                    tracing::warn!("Failed to batch initialize prices with local data: {:?}", e);
                    // 即使本地数据初始化失败，也继续执行，尝试从后端获取最新数据
                }
            }
        }

        // 发送任务到队列，获取最新数据并更新币价
        // PullApiWalletCoins任务成功后会用最新数据覆盖之前的本地数据
        Tasks::new().push(InitializationTask::PullApiWalletCoins).send().await?;

        Ok(())
    }

    pub(crate) async fn upsert_hot_coin_list(
        coins: Vec<ApiCoinData>,
    ) -> Result<Vec<ApiCoinEntity>, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
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

        let res = ApiCoinRepo::upsert_multi_coin(&pool, coin_data).await?;
        Ok(res)
    }

    pub async fn pull_api_coins() -> Result<Vec<ApiCoinEntity>, crate::error::service::ServiceError>
    {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // 删除掉无效的token
        ApiCoinRepo::drop_coin_just_null_token_address(&pool).await?;

        // 拉所有的币
        let coins = ApiCoinDomain::fetch_all_coin().await?;
        let data =
            coins.into_iter().map(|d| coin_info_to_coin_data(d)).collect::<Vec<ApiCoinData>>();

        let res = ApiCoinDomain::upsert_hot_coin_list(data).await?;

        Ok(res)
    }

    /// 查询代币汇率
    pub async fn get_api_token_currencies()
    -> Result<TokenCurrencies, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let currency = ConfigDomain::get_currency().await?;

        let coins = ApiCoinRepo::coin_list_v2(&pool, None, None).await?;

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
                decimals: coin.decimals,
            };
            map.insert(token_currency_id, token_currency);
        }

        Ok(TokenCurrencies(map))
    }

    pub(crate) fn merge_coin_to_list(
        coins: Vec<ApiCoinEntity>,
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
                data.push(crate::response_vo::standard_wallet::coin::CoinInfo {
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

    pub async fn fetch_all_coin() -> Result<Vec<ApiCoinInfo>, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // 本地没有币拉服务端所有的币,有拉去创建时间后的币种
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut coins = Vec::new();

        // TODO 1.5 版本验证币数量如果大于500说明已经同步过最新的币了,拉最新的。
        // let create_at = None;
        let count = ApiCoinRepo::coin_count(&pool).await?;
        let create_at = if count > 500 {
            if let Some(last_coin) = ApiCoinRepo::last_coin(&pool, true).await? {
                let formatted = last_coin.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
                Some(formatted)
            } else {
                None
            }
        } else {
            None
        };

        coins.append(&mut backend_api.fetch_all_api_tokens(create_at.clone(), None).await?);

        Ok(coins)
    }

    pub async fn init_token_price() -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let update_at = if let Some(last_coin) = ApiCoinRepo::last_coin(&pool, false).await? {
            last_coin.updated_at.map(|s| s.format("%Y-%m-%d %H:%M:%S").to_string())
        } else {
            None
        };

        let coins = backend_api.fetch_all_api_tokens(None, update_at).await?;

        for token in coins {
            let status = token.get_status();
            let time =
                // crate::infrastructure::parse_utc_with_error(&token.update_time.to_string()).ok();
                token.update_time;
            // 修复数据
            let sol_symbol = if token.token_address
                == Some("So11111111111111111111111111111111111111112".to_string())
            {
                token.symbol.clone()
            } else {
                None
            };

            let coin_id = wallet_database::entities::coin::CoinId {
                chain_code: token.chain_code.unwrap_or_default(),
                symbol: token.symbol.unwrap_or_default(),
                token_address: token.token_address.clone(),
            };

            ApiCoinRepo::update_price_unit(
                &coin_id,
                &token.price.unwrap_or_default().to_string(),
                token.decimals,
                status,
                time,
                sol_symbol,
                &pool,
            )
            .await?;
        }

        Ok(())
    }

    pub async fn add_supported_coin(
        coins: Vec<ApiCoinEntity>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let accounts = ApiAccountRepo::list(&pool).await?;

        for coin in coins {
            for account in accounts.iter() {
                if account.chain_code == coin.chain_code && coin.status == 1 {
                    // tracing::info!(
                    //     "add_supported_coin: chain_code: {}, symbol:{}",
                    //     account.chain_code,
                    //     coin.symbol
                    // );
                    let assets_id = AssetsId::new(
                        &account.address,
                        &account.chain_code,
                        &coin.symbol,
                        coin.token_address.clone(),
                    );
                    let assets =
                        ApiCreateAssetsVo::new(assets_id, coin.decimals, coin.protocol.clone(), 0)
                            .with_name(&coin.name)
                            .with_u256(alloy::primitives::U256::default(), coin.decimals)?;
                    ApiAssetsRepo::upsert_assets(&pool, assets).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn get_coin(
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
    ) -> Result<ApiCoinEntity, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let coin =
            ApiCoinRepo::coin_by_symbol_chain(chain_code, symbol, token_address, &pool).await?;

        Ok(coin)
    }
}

pub fn coin_info_to_coin_data(coin: ApiCoinInfo) -> ApiCoinData {
    ApiCoinData {
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
        created_at: coin.create_time,
        updated_at: coin.update_time,
    }
}
