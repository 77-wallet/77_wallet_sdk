use chrono::{DateTime, Utc};

use crate::{
    DbPool,
    dao::api_coin::ApiCoinDao,
    entities::{
        api_coin::{ApiCoinData, ApiCoinEntity},
        coin::{BatchCoinSwappable, CoinId, CoinWithAssets},
    },
    pagination::Pagination,
};

pub struct ApiCoinRepo;

impl ApiCoinRepo {
    pub async fn upsert_multi_coin(
        pool: &DbPool,
        coin: Vec<ApiCoinData>,
    ) -> Result<(), crate::Error> {
        ApiCoinDao::upsert_multi_coin(pool.as_ref(), coin).await
    }

    pub async fn coin_list(pool: &DbPool) -> Result<Vec<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::list(pool.as_ref(), None, None, None).await
    }

    pub async fn coin_list_v2(
        pool: &DbPool,
        symbol: Option<String>,
        chain_code: Option<String>,
    ) -> Result<Vec<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::list_v2(pool.as_ref(), symbol, chain_code, None).await
    }

    pub async fn coin_list_by_chain_token_map_batch(
        pool: &DbPool,
        chain_list: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::list_by_chain_token_map_batch(pool.as_ref(), chain_list).await
    }

    pub async fn coin_by_symbol_chain(
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        pool: &DbPool,
    ) -> Result<ApiCoinEntity, crate::Error> {
        ApiCoinDao::get_coin(chain_code, symbol, token_address, pool.as_ref()).await?.ok_or(
            crate::Error::NotFound(format!(
                "coin not found: chain_code: {}, symbol: {}",
                chain_code, symbol
            )),
        )
    }

    pub async fn main_coin(chain_code: &str, pool: &DbPool) -> Result<ApiCoinEntity, crate::Error> {
        ApiCoinDao::main_coin(chain_code, pool.as_ref()).await?.ok_or(crate::Error::NotFound(
            format!("main coin not found: chain_code: {}", chain_code),
        ))
    }

    pub async fn update_price_unit(
        coin_id: &CoinId,
        price: &str,
        unit: Option<u8>,
        status: Option<i32>,
        time: Option<DateTime<Utc>>,
        symbol: Option<String>,
        pool: &DbPool,
    ) -> Result<(), crate::Error> {
        ApiCoinDao::update_price_unit(pool.as_ref(), coin_id, price, unit, status, time, symbol)
            .await
    }

    pub async fn get_coin_by_chain_code_token_address(
        pool: &DbPool,
        chain_code: &str,
        token_address: &str,
    ) -> Result<Option<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::get_coin_by_chain_code_token_address(pool.as_ref(), chain_code, token_address)
            .await
    }

    pub async fn coin_list_symbol_not_in(
        pool: &DbPool,
        exclude: &[CoinId],
        chain_code: Option<String>,
        keyword: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<crate::pagination::Pagination<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::coin_list_symbol_not_in(pool, exclude, chain_code, keyword, page, page_size)
            .await
    }

    pub async fn update_price_unit1(
        chain_code: &str,
        token_address: &str,
        price: &str,
        pool: &DbPool,
    ) -> Result<(), crate::Error> {
        ApiCoinDao::update_price_unit1(pool.as_ref(), chain_code, token_address, price).await
    }

    pub async fn multi_update_swappable(
        coins: Vec<BatchCoinSwappable>,
        pool: &DbPool,
    ) -> Result<(), crate::Error> {
        ApiCoinDao::multi_update_swappable(coins, pool.as_ref()).await
    }

    pub async fn coin_by_chain_address(
        chain_code: &str,
        token_address: &str,
        pool: &DbPool,
    ) -> Result<ApiCoinEntity, crate::Error> {
        ApiCoinRepo::get_coin_by_chain_code_token_address(&pool, chain_code, token_address)
            .await?
            .ok_or(crate::Error::NotFound(format!(
                "coin not found: chain_code: {}, token: {}",
                chain_code, token_address,
            )))
    }

    pub async fn last_coin(
        pool: &DbPool,
        is_create: bool,
    ) -> Result<Option<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::get_last_coin(pool.as_ref(), is_create).await
    }

    pub async fn coin_count(pool: &DbPool) -> Result<i64, crate::Error> {
        ApiCoinDao::coin_count(pool.as_ref()).await
    }

    pub async fn same_coin_num(
        pool: &DbPool,
        symbol: &str,
        chain_code: &str,
    ) -> Result<i64, crate::Error> {
        ApiCoinDao::same_coin_num(pool.as_ref(), symbol, chain_code).await
    }

    pub async fn coin_list_with_assets(
        search: &str,
        exclude_token: Vec<String>,
        chain_code: String,
        address: Vec<String>,
        page: i64,
        page_size: i64,
        pool: DbPool,
    ) -> Result<Pagination<CoinWithAssets>, crate::Error> {
        ApiCoinDao::coin_list_with_assets(
            search,
            exclude_token,
            chain_code,
            address,
            page,
            page_size,
            pool,
        )
        .await
    }

    pub async fn drop_coin_just_null_token_address(pool: &DbPool) -> Result<(), crate::Error> {
        ApiCoinDao::drop_coin_just_null_token_address(pool.as_ref()).await
    }
}
