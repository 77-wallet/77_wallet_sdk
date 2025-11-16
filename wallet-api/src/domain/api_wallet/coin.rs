use chrono::{DateTime, Utc};
use wallet_database::{
    entities::api_coin::ApiCoinData, repositories::api_wallet::coin::ApiCoinRepo,
};
use wallet_transport_backend::response_vo::api_wallet::coin::ApiCoinInfo;

use crate::infrastructure::{
    parse_utc_datetime,
    task_queue::{initialization::InitializationTask, task::Tasks},
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
            price: Some("0".to_string()),
            status: if coin.active { 1 } else { 0 },
            created_at: DateTime::<Utc>::default(),
            updated_at: Some(DateTime::<Utc>::default()),
        }
    }
}
pub struct ApiCoinDomain {}

impl ApiCoinDomain {
    pub async fn init_api_coins() -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // check 本地表是否有数据,有则不进行新增
        let count = ApiCoinRepo::coin_count(&pool).await?;
        if count <= 0 {
            let list: Vec<ApiCoinData> = crate::default_data::coin::init_default_coins_list()?
                .coins
                .iter()
                .map(|coin| coin.to_owned().into())
                .collect();
            Self::upsert_hot_coin_list(list).await?;
        }

        let list = ApiCoinRepo::default_coin_list(&pool).await?;
        for coin in list.iter() {
            crate::infrastructure::asset_calc::update_token_price(
                &coin.symbol,
                &coin.chain_code,
                &coin.token_address,
                wallet_utils::unit::string_to_f64(&coin.price)?,
            )
            .await?;
        }
        crate::infrastructure::asset_calc::init_assets().await?;
        Tasks::new().push(InitializationTask::PullApiWalletCoins).send().await?;

        Ok(())
    }

    pub(crate) async fn upsert_hot_coin_list(
        coins: Vec<ApiCoinData>,
    ) -> Result<(), crate::error::service::ServiceError> {
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

        ApiCoinRepo::upsert_multi_coin(&pool, coin_data).await?;
        Ok(())
    }

    pub async fn pull_api_coins() -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // 删除掉无效的token
        ApiCoinRepo::drop_coin_just_null_token_address(&pool).await?;

        // 拉所有的币
        let coins = ApiCoinDomain::fetch_all_coin().await?;

        let data =
            coins.into_iter().map(|d| coin_info_to_coin_data(d)).collect::<Vec<ApiCoinData>>();

        ApiCoinDomain::upsert_hot_coin_list(data).await?;

        Ok(())
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
            let time = crate::infrastructure::parse_utc_with_error(&token.update_time).ok();

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
        created_at: parse_utc_datetime(&coin.create_time),
        updated_at: Some(parse_utc_datetime(&coin.update_time)),
    }
}
