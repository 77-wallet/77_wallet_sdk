// src/asset_calc.rs
use std::{sync::Arc, time::Duration};

use dashmap::{DashMap, DashSet};
use once_cell::sync::Lazy;
use rayon::prelude::*;
use sqlx::{Row, SqlitePool};
use tokio::sync::RwLock;

/// Key format for price lookup
fn make_key(symbol: &str, chain_code: &str, token_address: &str) -> String {
    format!("{}:{}:{}", symbol, chain_code, token_address)
}

fn make_asset_key(address: &str, chain_code: &str, token_address: &str) -> String {
    format!("{}:{}:{}", address, chain_code, token_address)
}

#[derive(Clone, Debug)]
pub struct PriceEntry {
    pub price: f64,
}

#[derive(Clone, Debug)]
pub struct AssetEntry {
    pub address: String,
    pub symbol: String,
    pub chain_code: String,
    pub token_address: String,
    pub balance: f64,
    pub decimals: i32,
}

static PRICE_CACHE: Lazy<DashMap<String, PriceEntry>> = Lazy::new(|| DashMap::new());
static DIRTY_PRICE_SET: Lazy<DashSet<String>> = Lazy::new(|| DashSet::new());
static ASSET_DIRTY_SET: Lazy<DashSet<String>> = Lazy::new(|| DashSet::new());
static ASSET_VALUE_CACHE: Lazy<DashMap<String, f64>> = Lazy::new(|| DashMap::new());
static TOTAL_USDT: Lazy<RwLock<f64>> = Lazy::new(|| RwLock::new(0.0));

/// Called when a new price arrives (price_real already as f64)
pub fn on_price_update(
    symbol: &str,
    chain_code: &str,
    token_address: &Option<String>,
    price_real: f64,
) {
    let key = make_key(symbol, chain_code, token_address.as_deref().unwrap_or(""));
    PRICE_CACHE.insert(key.clone(), PriceEntry { price: price_real });
    tracing::info!("on_price_update: {:?}", key);
    DIRTY_PRICE_SET.insert(key);
}

/// Called when a new asset is inserted or its balance changes
pub fn on_asset_update(address: &str, chain_code: &str, token_address: &str) {
    let k = make_asset_key(address, chain_code, token_address);
    tracing::info!("on_asset_update: {}", k);
    ASSET_DIRTY_SET.insert(k);
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
            let price_keys: Vec<String> = DIRTY_PRICE_SET.iter().map(|k| k.clone()).collect();
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

            // recompute total (simple reduction)
            let total: f64 = ASSET_VALUE_CACHE.iter().map(|kv| *kv.value()).sum();
            tracing::info!("batch recalculation finished, total: {:?}", total);
            if let Ok(mut t) = TOTAL_USDT.try_write() {
                *t = total;
            } else {
                let total_clone = total;
                tokio::spawn(async move {
                    let mut guard = TOTAL_USDT.write().await;
                    *guard = total_clone;
                });
            }

            tracing::info!(
                "batch recalculation finished: total_usdt={:.6}, cache_size={}",
                *TOTAL_USDT.read().await,
                ASSET_VALUE_CACHE.len()
            );
        }
    });
    Ok(())
}

async fn process_price_dirty_assets(pool: &Arc<SqlitePool>, keys: &[String]) {
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
            q = q.bind(k);
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
        // parallel compute
        assets.par_iter().for_each(|a| {
            let price_key = make_key(&a.symbol, &a.chain_code, &a.token_address);
            let asset_key = make_asset_key(&a.address, &a.chain_code, &a.token_address);
            let value = PRICE_CACHE
                .get(&price_key)
                .map(|p| (a.balance / 10f64.powi(a.decimals)) * p.price)
                .unwrap_or(0.0);
            ASSET_VALUE_CACHE.insert(asset_key, value);
        });
    }

    tracing::info!("batch recalculation finished, keys: {:?}", keys);
}

/// Handle asset dirty IDs
async fn process_asset_dirty_assets(pool: &Arc<SqlitePool>, keys: &[String]) {
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

        assets.par_iter().for_each(|a| {
            let price_key = make_key(&a.symbol, &a.chain_code, &a.token_address);
            let asset_key = make_asset_key(&a.address, &a.chain_code, &a.token_address);
            let value = PRICE_CACHE
                .get(&price_key)
                .map(|p| (a.balance / 10f64.powi(a.decimals)) * p.price)
                .unwrap_or(0.0);
            ASSET_VALUE_CACHE.insert(asset_key, value);
        });
    }
}

/// Force full refresh: runs a full join query and repopulates ASSET_VALUE_CACHE (accurate, but heavy)
pub async fn force_refresh_all_assets(db: Arc<SqlitePool>) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT a.address, a.symbol, a.chain_code, a.token_address, a.balance, a.decimals, c.price
        FROM api_assets a
        LEFT JOIN coin c
           ON a.symbol = c.symbol
          AND a.chain_code = c.chain_code
          AND a.token_address = c.token_address
        "#,
    )
    .fetch_all(db.as_ref())
    .await?;

    ASSET_VALUE_CACHE.clear();

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

    assets.par_iter().for_each(|a| {
        let price_key = make_key(&a.symbol, &a.chain_code, &a.token_address);
        let asset_key = make_asset_key(&a.address, &a.chain_code, &a.token_address);
        let price = PRICE_CACHE.get(&price_key).map(|p| p.price).unwrap_or(0.0);
        let value = (a.balance / 10f64.powi(a.decimals)) * price;
        ASSET_VALUE_CACHE.insert(asset_key, value);
    });

    let total: f64 = ASSET_VALUE_CACHE.iter().map(|kv| *kv.value()).sum();
    {
        let mut guard = TOTAL_USDT.write().await;
        *guard = total;
    }

    Ok(())
}

/// Get current total snapshot
pub async fn get_total_usdt() -> f64 {
    *TOTAL_USDT.read().await
}

/// Get current price cache
pub async fn get_price_cache() {
    tracing::info!("get_price_cache: {:#?}", PRICE_CACHE);
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
