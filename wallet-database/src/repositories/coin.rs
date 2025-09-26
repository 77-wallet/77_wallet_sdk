use alloy::signers::Result;
use chrono::{DateTime, Utc};

use crate::{
    DbPool,
    entities::coin::{BatchCoinSwappable, CoinData, CoinEntity, CoinId, CoinWithAssets, SymbolId},
    pagination::Pagination,
};

#[async_trait::async_trait]
pub trait CoinRepoTrait: super::TransactionTrait {
    async fn upsert_multi_coin(&mut self, coin: Vec<CoinData>) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, CoinEntity::upsert_multi_coin, coin)
    }

    async fn detail(
        &mut self,
        symbol: &str,
        chain_code: &str,
        token_address: Option<String>,
    ) -> Result<Option<CoinEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            CoinEntity::detail,
            symbol,
            chain_code,
            token_address
        )
    }

    // async fn coin_list(
    //     &mut self,
    //     symbol: Option<&str>,
    //     chain_code: Option<String>,
    // ) -> Result<Vec<CoinEntity>, crate::Error> {
    //     let executor = self.get_conn_or_tx()?;
    //     let symbol = if let Some(symbol) = symbol {
    //         vec![symbol.to_string()]
    //     } else {
    //         Vec::new()
    //     };
    //     crate::execute_with_executor!(executor, CoinEntity::list, &symbol, chain_code, None)
    // }

    async fn coin_list_v2(
        &mut self,
        symbol: Option<String>,
        chain_code: Option<String>,
    ) -> Result<Vec<CoinEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, CoinEntity::list_v2, symbol, chain_code, None)
    }

    async fn coin_list_by_chain_token_map_batch(
        &mut self,
        pool: &DbPool,
        chain_list: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<CoinEntity>, crate::Error> {
        CoinEntity::list_by_chain_token_map_batch(pool.as_ref(), chain_list).await
    }

    async fn get_coin_by_chain_code_token_address(
        &mut self,
        chain_code: &str,
        token_address: &str,
    ) -> Result<Option<CoinEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            CoinEntity::get_coin_by_chain_code_token_address,
            chain_code,
            token_address
        )
    }

    async fn coin_list_with_symbols(
        &mut self,
        symbols: &[String],
        chain_code: Option<String>,
    ) -> Result<Vec<CoinEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, CoinEntity::list, symbols, chain_code, None)
    }

    async fn default_coin_list(&mut self) -> Result<Vec<CoinEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, CoinEntity::list_v2, None, None, Some(1))
    }

    async fn get_market_chain_list(&mut self) -> Result<Vec<String>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, CoinEntity::chain_code_list,)
    }

    // async fn symbol_list(
    //     &mut self,
    //     chain_code: Option<String>,
    // ) -> Result<Vec<Coins>, crate::Error> {
    //     let executor = self.get_conn_or_tx()?;
    //     crate::execute_with_executor!(executor, CoinEntity::symbol_list, chain_code)
    // }

    async fn hot_coin_list_symbol_not_in(
        &mut self,
        exclude: &[CoinId],
        chain_code: Option<String>,
        keyword: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<crate::pagination::Pagination<CoinEntity>, crate::Error> {
        let executor = self.get_db_pool();
        CoinEntity::coin_list_symbol_not_in(executor, exclude, chain_code, keyword, page, page_size)
            .await
    }

    async fn update_price_unit(
        &mut self,
        coin_id: &CoinId,
        price: &str,
        unit: Option<u8>,
        status: Option<i32>,
        swappable: Option<bool>,
        time: Option<DateTime<Utc>>,
    ) -> Result<Vec<CoinEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            CoinEntity::update_price_unit,
            coin_id,
            price,
            unit,
            status,
            swappable,
            time
        )
    }

    async fn drop_coin_just_null_token_address(&mut self) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, CoinEntity::drop_coin_just_null_token_address,)
    }

    async fn drop_custom_coin(&mut self, coin_id: &SymbolId) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, CoinEntity::drop_custom_coin, coin_id)
    }

    async fn drop_multi_custom_coin(
        &mut self,
        coin_ids: std::collections::HashSet<SymbolId>,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, CoinEntity::drop_multi_custom_coin, coin_ids)
    }

    async fn clean_table(&mut self) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, CoinEntity::clean_table,)
    }
}

pub struct CoinRepo;
impl CoinRepo {
    pub async fn coin_by_symbol_chain(
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        pool: &DbPool,
    ) -> Result<CoinEntity, crate::Error> {
        CoinEntity::get_coin(chain_code, symbol, token_address, pool.as_ref())
            .await?
            .ok_or(crate::Error::NotFound(format!(
                "coin not found: chain_code: {}, symbol: {}",
                chain_code, symbol
            )))
    }

    pub async fn main_coin(chain_code: &str, pool: &DbPool) -> Result<CoinEntity, crate::Error> {
        CoinEntity::main_coin(chain_code, pool.as_ref())
            .await?
            .ok_or(crate::Error::NotFound(format!(
                "main coin not found: chain_code: {}",
                chain_code
            )))
    }

    // 修复数据用
    pub async fn delete_wsol_error(pool: &DbPool) -> Result<(), crate::Error> {
        CoinEntity::delete_wsol_error(pool.as_ref()).await
    }

    pub async fn update_price_unit1(
        chain_code: &str,
        token_address: &str,
        price: &str,
        pool: &DbPool,
    ) -> Result<(), crate::Error> {
        CoinEntity::update_price_unit1(pool.as_ref(), chain_code, token_address, price).await
    }

    pub async fn multi_update_swappable(
        coins: Vec<BatchCoinSwappable>,
        pool: &DbPool,
    ) -> Result<(), crate::Error> {
        CoinEntity::multi_update_swappable(coins, pool.as_ref()).await
    }

    pub async fn coin_by_chain_address(
        chain_code: &str,
        token_address: &str,
        pool: &DbPool,
    ) -> Result<CoinEntity, crate::Error> {
        CoinEntity::get_coin_by_chain_code_token_address(pool.as_ref(), chain_code, token_address)
            .await?
            .ok_or(crate::Error::NotFound(format!(
                "coin not found: chain_code: {}, token: {}",
                chain_code, token_address,
            )))
    }

    pub async fn last_coin(
        pool: &DbPool,
        is_create: bool,
    ) -> Result<Option<CoinEntity>, crate::Error> {
        CoinEntity::get_last_coin(pool.as_ref(), is_create).await
    }

    pub async fn coin_count(pool: &DbPool) -> Result<i64, crate::Error> {
        CoinEntity::coin_count(pool.as_ref()).await
    }

    pub async fn same_coin_num(
        pool: &DbPool,
        symbol: &str,
        chain_code: &str,
    ) -> Result<i64, crate::Error> {
        CoinEntity::same_coin_num(pool.as_ref(), symbol, chain_code).await
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
        CoinEntity::coin_list_with_assets(
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
}
