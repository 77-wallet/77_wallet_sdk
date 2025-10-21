// src/asset_calc.rs
use std::{collections::HashMap, sync::Arc, time::Duration};

use dashmap::{DashMap, DashSet};
use once_cell::sync::Lazy;
use rayon::prelude::*;
use sqlx::{Row, SqlitePool};
use tokio::sync::RwLock;
use wallet_database::repositories::{
    api_wallet::{account::ApiAccountRepo, assets::ApiAssetsRepo},
    coin::CoinRepo,
    exchange_rate::ExchangeRateRepo,
};
use wallet_transport_backend::response_vo::coin::TokenCurrency;

use crate::{
    domain::app::config::ConfigDomain,
    response_vo::{
        account::BalanceInfo,
        coin::{TokenCurrencies, TokenCurrencyId},
    },
};

/// Key format for price lookup

fn make_asset_key(address: &str, chain_code: &str, token_address: &str) -> String {
    format!("{}:{}:{}", address, chain_code, token_address)
}

// #[derive(Clone, Debug)]
// pub struct PriceEntry {
//     pub price: f64,
// }

#[derive(Clone, Debug)]
pub struct AssetEntry {
    pub address: String,
    pub symbol: String,
    pub chain_code: String,
    pub token_address: String,
    pub balance: f64,
    pub decimals: i32,
}

static TOKEN_CURRENCIES: Lazy<Arc<RwLock<TokenCurrencies>>> =
    Lazy::new(|| Arc::new(RwLock::new(TokenCurrencies::default())));
// static PRICE_CACHE: Lazy<DashMap<String, PriceEntry>> = Lazy::new(|| DashMap::new());
static DIRTY_PRICE_SET: Lazy<DashSet<TokenCurrencyId>> = Lazy::new(|| DashSet::new());
static ASSET_DIRTY_SET: Lazy<DashSet<String>> = Lazy::new(|| DashSet::new());
// static ASSET_DIRTY_SET: Lazy<DashSet<TokenCurrencyId>> = Lazy::new(|| DashSet::new());

static ASSET_VALUE_CACHE: Lazy<DashMap<String, BalanceInfo>> = Lazy::new(|| DashMap::new());
static TOTAL_USDT: Lazy<RwLock<f64>> = Lazy::new(|| RwLock::new(0.0));

pub async fn update_token_price(
    symbol: &str,
    chain_code: &str,
    token_address: &Option<String>,
    price_real: f64,
) -> Result<(), crate::error::service::ServiceError> {
    let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
    let mut token_currencies = TOKEN_CURRENCIES.write().await;
    let id = TokenCurrencyId::new(symbol, chain_code, token_address.clone());
    let currency = ConfigDomain::get_currency().await?;

    let (fiat_price, rate) = {
        let exchange_rate_list = ExchangeRateRepo::list(&pool).await?;
        if let Some(rate) = exchange_rate_list.iter().find(|rate| rate.target_currency == currency)
        {
            (Some(price_real * rate.rate), rate.rate)
        } else {
            (None, 1.0)
        }
    };

    // 更新缓存
    token_currencies
        .entry(id.clone())
        .and_modify(|entry| {
            entry.price = Some(price_real);
            entry.currency_price = fiat_price;
            entry.rate = rate;
        })
        .or_insert(TokenCurrency::new(chain_code, symbol, "", Some(price_real), fiat_price, rate));

    // 标记 dirty，用于触发资产估值刷新
    DIRTY_PRICE_SET.insert(id);

    Ok(())
}

/// Called when a new asset is inserted or its balance changes
pub fn on_asset_update(address: &str, chain_code: &str, token_address: &str) {
    let k = make_asset_key(address, chain_code, token_address);
    tracing::info!("on_asset_update: {}", k);
    ASSET_DIRTY_SET.insert(k);
}

pub async fn init_assets() -> Result<(), crate::error::service::ServiceError> {
    let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
    let list = ApiAssetsRepo::list(&pool, vec![], None).await?;
    tracing::info!("init_assets: list: {list:#?}");
    list.into_iter().for_each(|asset| {
        on_asset_update(&asset.address, &asset.chain_code, &asset.token_address);
    });

    Ok(())
}

/// Start the periodic batch recalculation background task.
/// interval_ms: how often to run the batch recalculation (e.g. 500 or 1000)
pub fn start_batch_recalculator(
    interval_ms: u64,
) -> Result<(), crate::error::service::ServiceError> {
    let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
    tokio::spawn(async move {
        let interval = Duration::from_millis(interval_ms);
        loop {
            tokio::time::sleep(interval).await;

            // --- collect dirty sets ---
            let price_keys: Vec<TokenCurrencyId> =
                DIRTY_PRICE_SET.iter().map(|k| k.clone()).collect();
            let asset_keys: Vec<String> = ASSET_DIRTY_SET.iter().map(|id| id.clone()).collect();

            if price_keys.is_empty() && asset_keys.is_empty() {
                continue;
            }

            tracing::info!(
                "batch recalculation started: price_keys={}, asset_ids={}",
                price_keys.len(),
                asset_keys.len()
            );

            // clear old dirty marks
            for k in &price_keys {
                DIRTY_PRICE_SET.remove(k);
            }
            for id in &asset_keys {
                ASSET_DIRTY_SET.remove(id);
            }

            if !price_keys.is_empty() {
                process_price_dirty_assets(&pool, &price_keys).await;
            }

            if !asset_keys.is_empty() {
                process_asset_dirty_assets(&pool, &asset_keys).await;
            }

            // // recompute total (simple reduction)
            // let total: f64 = ASSET_VALUE_CACHE.iter().map(|kv| *kv.value()).sum();
            // tracing::info!("batch recalculation finished, total: {:?}", total);
            // if let Ok(mut t) = TOTAL_USDT.try_write() {
            //     *t = total;
            // } else {
            //     let total_clone = total;
            //     tokio::spawn(async move {
            //         let mut guard = TOTAL_USDT.write().await;
            //         *guard = total_clone;
            //     });
            // }

            // tracing::info!(
            //     "batch recalculation finished: total_usdt={:.6}, cache_size={}",
            //     *TOTAL_USDT.read().await,
            //     ASSET_VALUE_CACHE.len()
            // );
        }
    });
    Ok(())
}

async fn process_price_dirty_assets(
    pool: &Arc<SqlitePool>,
    keys: &[TokenCurrencyId],
) -> Result<(), Box<dyn std::error::Error>> {
    // process in chunks to avoid huge IN lists
    const CHUNK_KEYS: usize = 200;
    for chunk in keys.chunks(CHUNK_KEYS) {
        let mut query = String::from(
            "SELECT address, symbol, chain_code, token_address, balance, decimals FROM api_assets WHERE (symbol || ':' || chain_code || ':' || token_address) IN (",
        );
        query.push_str(&chunk.iter().map(|_| "?").collect::<Vec<_>>().join(","));
        query.push(')');

        tracing::info!("batch query: {}", query);
        let mut q = sqlx::query(&query);
        for k in chunk {
            q = q.bind(k.gen_key());
        }

        let rows = match q.fetch_all(pool.as_ref()).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("batch query error: {}", e);
                continue;
            }
        };
        tracing::info!("batch query result: {:?}", rows.len());
        let assets: Vec<AssetEntry> = rows
            .into_iter()
            .map(|r| AssetEntry {
                address: r.get("address"),
                symbol: r.get("symbol"),
                chain_code: r.get("chain_code"),
                token_address: r.get("token_address"),
                balance: r
                    .get::<Option<String>, _>("balance")
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0),
                decimals: r.get("decimals"),
            })
            .collect();

        tracing::info!("batch recalculation finished, assets: {:?}", assets);

        let token_currencies_snapshot = {
            let guard = TOKEN_CURRENCIES.read().await;
            guard.clone()
        };
        let currency = ConfigDomain::get_currency().await?;

        // parallel compute
        assets.par_iter().for_each(|a| {
            // let price_key = TokenCurrencyId::make_key(&a.symbol, &a.chain_code, &a.token_address);
            let asset_key = make_asset_key(&a.address, &a.chain_code, &a.token_address);
            let balance_info = token_currencies_snapshot
                .calculate_sync_to_balance(
                    &currency,
                    &a.balance.to_string(),
                    &a.symbol,
                    &a.chain_code,
                    Some(a.token_address.clone()),
                )
                .unwrap_or(BalanceInfo {
                    amount: 0.0,
                    currency: "".to_string(),
                    unit_price: None,
                    fiat_value: None,
                });

            ASSET_VALUE_CACHE.insert(asset_key, balance_info);
        });
    }

    tracing::info!("batch recalculation finished, keys: {:?}", keys);
    Ok(())
}

/// Handle asset dirty IDs
async fn process_asset_dirty_assets(
    pool: &Arc<SqlitePool>,
    keys: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    const CHUNK_SIZE: usize = 200;
    for chunk in keys.chunks(CHUNK_SIZE) {
        let mut query = String::from(
            "SELECT address, symbol, chain_code, token_address, balance, decimals \
             FROM api_assets WHERE (address || ':' || chain_code || ':' || token_address) IN (",
        );
        query.push_str(&chunk.iter().map(|_| "?").collect::<Vec<_>>().join(","));
        query.push(')');

        let mut q = sqlx::query(&query);
        for key in chunk {
            q = q.bind(key);
        }

        let rows = match q.fetch_all(pool.as_ref()).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("asset_dirty query error: {}", e);
                continue;
            }
        };

        let assets: Vec<AssetEntry> = rows
            .into_iter()
            .map(|row| AssetEntry {
                address: row.get("address"),
                symbol: row.get("symbol"),
                chain_code: row.get("chain_code"),
                token_address: row.get("token_address"),
                balance: row
                    .get::<Option<String>, _>("balance")
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0),
                decimals: row.get("decimals"),
            })
            .collect();

        let token_currencies_snapshot = {
            let guard = TOKEN_CURRENCIES.read().await;
            guard.clone()
        };
        let currency = ConfigDomain::get_currency().await?;

        assets.par_iter().for_each(|a| {
            let id = TokenCurrencyId::new(&a.symbol, &a.chain_code, Some(a.token_address.clone()));
            // let price_key = make_key(&a.symbol, &a.chain_code, &a.token_address);
            let asset_key = make_asset_key(&a.address, &a.chain_code, &a.token_address);
            let balance_info = token_currencies_snapshot
                .calculate_sync_to_balance(
                    &currency,
                    &a.balance.to_string(),
                    &a.symbol,
                    &a.chain_code,
                    Some(a.token_address.clone()),
                )
                .unwrap_or(BalanceInfo {
                    amount: 0.0,
                    currency: "".to_string(),
                    unit_price: None,
                    fiat_value: None,
                });
            ASSET_VALUE_CACHE.insert(asset_key, balance_info);
        });
    }

    Ok(())
}

// /// Force full refresh: runs a full join query and repopulates ASSET_VALUE_CACHE (accurate, but heavy)
// pub async fn force_refresh_all_assets(db: Arc<SqlitePool>) -> Result<(), sqlx::Error> {
//     let rows = sqlx::query(
//         r#"
//         SELECT a.address, a.symbol, a.chain_code, a.token_address, a.balance, a.decimals, c.price
//         FROM api_assets a
//         LEFT JOIN coin c
//            ON a.symbol = c.symbol
//           AND a.chain_code = c.chain_code
//           AND a.token_address = c.token_address
//         "#,
//     )
//     .fetch_all(db.as_ref())
//     .await?;

//     ASSET_VALUE_CACHE.clear();

//     let assets: Vec<AssetEntry> = rows
//         .into_iter()
//         .map(|r| AssetEntry {
//             address: r.get("address"),
//             symbol: r.get("symbol"),
//             chain_code: r.get("chain_code"),
//             token_address: r.get("token_address"),
//             balance: r
//                 .get::<Option<String>, _>("balance")
//                 .and_then(|s| s.parse::<f64>().ok())
//                 .unwrap_or(0.0),
//             decimals: r.get("decimals"),
//         })
//         .collect();

//     let token_currencies_snapshot = {
//         let guard = TOKEN_CURRENCIES.read().await;
//         guard.clone()
//     };
//     let currency = ConfigDomain::get_currency().await?;

//     assets.par_iter().for_each(|a| {
//         let id = TokenCurrencyId::new(&a.symbol, &a.chain_code, Some(a.token_address.clone()));
//         let asset_key = make_asset_key(&a.address, &a.chain_code, &a.token_address);
//         let price = token_currencies_snapshot.get(&id).map(|p| p.price).unwrap_or(0.0);
//         let value = (a.balance / 10f64.powi(a.decimals)) * price;
//         ASSET_VALUE_CACHE.insert(asset_key, value);
//     });

//     let total: f64 = ASSET_VALUE_CACHE.iter().map(|kv| *kv.value()).sum();
//     {
//         let mut guard = TOTAL_USDT.write().await;
//         *guard = total;
//     }

//     Ok(())
// }

/// Get current total snapshot
pub async fn get_total_usdt() -> f64 {
    *TOTAL_USDT.read().await
}

/// Get current price cache
pub async fn get_price_cache() {
    tracing::info!("get_price_cache: {:#?}", TOKEN_CURRENCIES);
    // let g = PRICE_CACHE.read().await;
    // g.clone()
}

// /// Get a page of asset snapshot (id and usdt_value)
// /// This implementation fetches asset ids from DB by page, then maps to cached values.
// pub async fn get_asset_snapshot_page(
//     page: usize,
//     page_size: usize,
// ) -> Result<Vec<(i64, f64)>, crate::error::service::ServiceError> {
//     let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
//     let offset = ((page.saturating_sub(1) * page_size) as i64).max(0);
//     let rows = sqlx::query("SELECT id FROM api_assets ORDER BY id LIMIT ? OFFSET ?")
//         .bind(page_size as i64)
//         .bind(offset)
//         .fetch_all(pool.as_ref())
//         .await
//         .unwrap();

//     let mut out = Vec::with_capacity(rows.len());
//     for row in rows {
//         let id: i64 = row.get("id");
//         let value = ASSET_VALUE_CACHE.get(&id).map(|v| *v.value()).unwrap_or(0.0);
//         out.push((id, value));
//     }
//     Ok(out)
// }

pub async fn get_wallet_balance_list()
-> Result<HashMap<String, BalanceInfo>, crate::error::service::ServiceError> {
    let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

    // 1️⃣ 获取账户与钱包的映射
    // account_address -> wallet_address
    let list = ApiAccountRepo::account_to_wallet(&pool).await?;

    let mut account_to_wallet: HashMap<String, String> = HashMap::new();
    for row in list {
        account_to_wallet.insert(row.address, row.wallet_address);
    }

    // 2️⃣ 聚合计算钱包总余额
    let mut wallet_totals: HashMap<String, BalanceInfo> = HashMap::new();

    tracing::info!("get_wallet_balance_list: {:#?}", ASSET_VALUE_CACHE);
    for entry in ASSET_VALUE_CACHE.iter() {
        if let Some(address) = entry.key().split(':').next() {
            tracing::info!("entry value: {}", address);
            if let Some(wallet_address) = account_to_wallet.get(address) {
                tracing::info!("get_wallet_balance_list: wallet_address: {:?}", wallet_address);
                let entry_value = entry.value();
                tracing::info!("get_wallet_balance_list amount: {}", entry_value.amount);
                wallet_totals
                    .entry(wallet_address.clone())
                    .and_modify(|total| {
                        total.amount_add(entry_value.amount);
                        total.fiat_add(entry_value.fiat_value);
                    })
                    .or_insert_with(|| entry_value.clone());
                // 👆 用 or_insert_with + clone()，因为 entry_value 是引用
            }
        }
    }

    Ok(wallet_totals)
}

pub async fn get_account_balance_list_by_wallet(
    wallet_address: &str,
) -> Result<HashMap<String, BalanceInfo>, crate::error::service::ServiceError> {
    let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

    // 1️⃣ 获取指定钱包下的所有账户（address -> wallet_address）
    let list = ApiAccountRepo::account_to_wallet(&pool).await?;

    // 过滤出属于当前 wallet 的账户
    let mut account_addresses: Vec<String> = Vec::new();
    for row in list {
        if row.wallet_address == wallet_address {
            account_addresses.push(row.address);
        }
    }

    if account_addresses.is_empty() {
        return Ok(HashMap::new());
    }

    // 2️⃣ 聚合计算每个账户的资产总额
    let mut account_totals: HashMap<String, BalanceInfo> = HashMap::new();

    for entry in ASSET_VALUE_CACHE.iter() {
        if let Some(address) = entry.key().split(':').next() {
            if account_addresses.contains(&address.to_string()) {
                let entry_value = entry.value();
                account_totals
                    .entry(address.to_string())
                    .and_modify(|total| {
                        total.amount_add(entry_value.amount);
                        total.fiat_add(entry_value.fiat_value);
                    })
                    .or_insert_with(|| entry_value.clone());
            }
        }
    }

    Ok(account_totals)
}
