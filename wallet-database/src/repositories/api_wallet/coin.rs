use chrono::{DateTime, Utc};

use crate::{
    ApiWalletDbPool,
    dao::api_coin::ApiCoinDao,
    entities::{
        api_coin::{ApiCoinData, ApiCoinEntity},
        asset_token_key::AssetTokenKey,
        coin::{BatchCoinSwappable, CoinId, CoinWithAssets},
    },
    pagination::Pagination,
};

pub struct ApiCoinRepo;

impl ApiCoinRepo {
    pub async fn upsert_multi_coin(
        pool: &ApiWalletDbPool,
        coin: Vec<ApiCoinData>,
    ) -> Result<Vec<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::upsert_multi_coin(pool.write_ref(), coin).await
    }

    pub async fn coin_list(pool: &ApiWalletDbPool) -> Result<Vec<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::list(pool.read_ref(), None, None, None).await
    }

    pub async fn coin_list_v2(
        pool: &ApiWalletDbPool,
        symbol: Option<String>,
        chain_code: Option<String>,
    ) -> Result<Vec<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::list_v2(pool.read_ref(), symbol, chain_code, None).await
    }

    pub async fn coin_list_by_chain_token_map_batch(
        pool: &ApiWalletDbPool,
        chain_list: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::list_by_chain_token_map_batch(pool.read_ref(), chain_list).await
    }

    pub async fn coin_list_by_chain_token_pairs_batch(
        pool: &ApiWalletDbPool,
        pairs: &[(String, String)],
    ) -> Result<Vec<ApiCoinEntity>, crate::Error> {
        if pairs.is_empty() {
            return Ok(vec![]);
        }

        // SQLite 默认参数上限通常为 999，每个 pair 需要 2 个 bind 参数
        const MAX_PAIRS_PER_QUERY: usize = 400;
        let mut all = Vec::new();
        for chunk in pairs.chunks(MAX_PAIRS_PER_QUERY) {
            let mut res =
                ApiCoinDao::list_by_chain_token_pairs_batch(pool.read_ref(), chunk).await?;
            all.append(&mut res);
        }
        Ok(all)
    }

    pub async fn main_coin(
        chain_code: &str,
        pool: &ApiWalletDbPool,
    ) -> Result<ApiCoinEntity, crate::Error> {
        ApiCoinDao::main_coin(chain_code, pool.read_ref()).await?.ok_or(crate::Error::NotFound(
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
        pool: &ApiWalletDbPool,
    ) -> Result<(), crate::Error> {
        ApiCoinDao::update_price_unit(pool.write_ref(), coin_id, price, unit, status, time, symbol)
            .await
    }

    pub async fn get_coin_by_chain_code_token_address(
        pool: &ApiWalletDbPool,
        chain_code: &str,
        token_address: &str,
    ) -> Result<Option<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::get_coin_by_chain_code_token_address(
            pool.read_ref(),
            chain_code,
            AssetTokenKey::from_raw(Some(token_address)),
        )
        .await
    }

    pub async fn coin_by_chain_token_key_opt(
        chain_code: &str,
        token_key: AssetTokenKey,
        pool: &ApiWalletDbPool,
    ) -> Result<Option<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::get_coin_by_chain_code_token_address(pool.read_ref(), chain_code, token_key)
            .await
    }

    pub async fn coin_list_symbol_not_in(
        pool: &ApiWalletDbPool,
        exclude: &[CoinId],
        chain_code: Option<String>,
        keyword: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<crate::pagination::Pagination<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::coin_list_symbol_not_in(
            pool.read_ref(),
            exclude,
            chain_code,
            keyword,
            page,
            page_size,
        )
        .await
    }

    pub async fn update_price_unit1(
        chain_code: &str,
        token_address: &str,
        price: &str,
        pool: &ApiWalletDbPool,
    ) -> Result<(), crate::Error> {
        ApiCoinDao::update_price_unit1(pool.write_ref(), chain_code, token_address, price).await
    }

    pub async fn multi_update_swappable(
        coins: Vec<BatchCoinSwappable>,
        pool: &ApiWalletDbPool,
    ) -> Result<(), crate::Error> {
        ApiCoinDao::multi_update_swappable(coins, pool.write_ref()).await
    }

    pub async fn coin_by_chain_token_key(
        chain_code: &str,
        token_key: AssetTokenKey,
        pool: &ApiWalletDbPool,
    ) -> Result<ApiCoinEntity, crate::Error> {
        let token_for_log = token_key.as_db_str().to_string();
        ApiCoinDao::get_coin_by_chain_code_token_address(pool.read_ref(), chain_code, token_key)
            .await?
            .ok_or(crate::Error::NotFound(format!(
                "coin not found: chain_code: {}, token: {}",
                chain_code, token_for_log
            )))
    }

    pub async fn has_coin(
        chain_code: &str,
        token_address: &str,
        pool: &ApiWalletDbPool,
    ) -> Result<bool, crate::Error> {
        ApiCoinRepo::get_coin_by_chain_code_token_address(&pool, chain_code, token_address)
            .await?
            .map_or_else(|| Ok(false), |_| Ok(true))
    }

    pub async fn last_coin(
        pool: &ApiWalletDbPool,
        is_create: bool,
    ) -> Result<Option<ApiCoinEntity>, crate::Error> {
        ApiCoinDao::get_last_coin(pool.read_ref(), is_create).await
    }

    pub async fn coin_count(pool: &ApiWalletDbPool) -> Result<i64, crate::Error> {
        ApiCoinDao::coin_count(pool.read_ref()).await
    }

    pub async fn same_coin_num(
        pool: &ApiWalletDbPool,
        symbol: &str,
        chain_code: &str,
    ) -> Result<i64, crate::Error> {
        ApiCoinDao::same_coin_num(pool.read_ref(), symbol, chain_code).await
    }

    pub async fn coin_list_with_assets(
        search: &str,
        exclude_token: Vec<String>,
        chain_code: String,
        address: Vec<String>,
        page: i64,
        page_size: i64,
        pool: &ApiWalletDbPool,
    ) -> Result<Pagination<CoinWithAssets>, crate::Error> {
        ApiCoinDao::coin_list_with_assets(
            search,
            exclude_token,
            chain_code,
            address,
            page,
            page_size,
            pool.read_pool(),
        )
        .await
    }

    pub async fn drop_coin_just_null_token_address(
        pool: &ApiWalletDbPool,
    ) -> Result<(), crate::Error> {
        ApiCoinDao::drop_coin_just_null_token_address(pool.write_ref()).await
    }
}

#[cfg(test)]
mod tests {
    use super::ApiCoinRepo;
    use crate::{
        dao::api_coin::ApiCoinDao, entities::api_coin::ApiCoinData,
        repositories::test_helper::setup_api_wallet_pool,
    };
    use chrono::Utc;

    fn make_coin(chain_code: &str, token_address: &str, price: &str) -> ApiCoinData {
        ApiCoinData::new(
            Some("USDT".to_string()),
            "USDT",
            chain_code,
            Some(token_address.to_string()).into(),
            Some(price.to_string()),
            None,
            6,
            1,
            1,
            1,
            Utc::now(),
            None,
        )
    }

    #[tokio::test]
    async fn coin_repo_upsert_and_get_success() {
        let pool = setup_api_wallet_pool("wallet_db_api_coin_success").await;
        let chain = wallet_types::constant::chain_code::ETHEREUM;
        let token = "0xapi_coin_token_s";
        let price = "1.23";

        ApiCoinRepo::upsert_multi_coin(&pool, vec![make_coin(chain, token, price)]).await.unwrap();

        let got =
            ApiCoinRepo::get_coin_by_chain_code_token_address(&pool, chain, token).await.unwrap();
        assert!(got.is_some());
        let got = got.unwrap();
        assert_eq!(got.chain_code, chain);
        assert_eq!(got.token_address.as_db_str(), token);
        assert_eq!(got.price, price);

        let count = ApiCoinRepo::coin_count(&pool).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn coin_repo_missing_token_returns_none() {
        let pool = setup_api_wallet_pool("wallet_db_api_coin_edge").await;
        let got = ApiCoinRepo::get_coin_by_chain_code_token_address(
            &pool,
            wallet_types::constant::chain_code::ETHEREUM,
            "0xapi_coin_missing_token",
        )
        .await
        .unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn coin_repo_tx_rollback_keeps_price_unchanged() {
        let pool = setup_api_wallet_pool("wallet_db_api_coin_rollback").await;
        let chain = wallet_types::constant::chain_code::ETHEREUM;
        let token = "0xapi_coin_token_rb";

        ApiCoinRepo::upsert_multi_coin(&pool, vec![make_coin(chain, token, "2.00")]).await.unwrap();

        let mut tx = pool.write_ref().begin().await.unwrap();
        ApiCoinDao::update_price_unit1(tx.as_mut(), chain, token, "9.99").await.unwrap();
        tx.rollback().await.unwrap();

        let got =
            ApiCoinRepo::get_coin_by_chain_code_token_address(&pool, chain, token).await.unwrap();
        assert_eq!(got.unwrap().price, "2.00");

        let count = ApiCoinRepo::coin_count(&pool).await.unwrap();
        assert_eq!(count, 1);
    }
}
