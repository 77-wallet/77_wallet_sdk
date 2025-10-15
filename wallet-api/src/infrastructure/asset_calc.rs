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

#[derive(Clone, Debug)]
pub struct PriceEntry {
    pub price: f64,
}

#[derive(Clone, Debug)]
pub struct AssetEntry {
    pub id: i64,
    pub symbol: String,
    pub chain_code: String,
    pub token_address: String,
    pub balance: f64, // already parsed to f64
    pub decimals: i32,
}

static PRICE_CACHE: Lazy<DashMap<String, PriceEntry>> = Lazy::new(|| DashMap::new());
static DIRTY_SET: Lazy<DashSet<String>> = Lazy::new(|| DashSet::new());
static ASSET_VALUE_CACHE: Lazy<DashMap<i64, f64>> = Lazy::new(|| DashMap::new());
static TOTAL_USDT: Lazy<RwLock<f64>> = Lazy::new(|| RwLock::new(0.0));

/// Called when a new price arrives (price_real already as f64)
pub fn on_price_update(symbol: &str, chain_code: &str, token_address: &str, price_real: f64) {
    let key = make_key(symbol, chain_code, token_address);
    PRICE_CACHE.insert(key.clone(), PriceEntry { price: price_real });
    DIRTY_SET.insert(key);
}

/// Start the periodic batch recalculation background task.
/// interval_ms: how often to run the batch recalculation (e.g. 500 or 1000)
pub fn start_batch_recalculator(db: Arc<SqlitePool>, interval_ms: u64) {
    tokio::spawn(async move {
        let interval = Duration::from_millis(interval_ms);
        loop {
            tokio::time::sleep(interval).await;

            // collect dirty keys
            let keys: Vec<String> = DIRTY_SET.iter().map(|k| k.clone()).collect();
            if keys.is_empty() {
                continue;
            }

            // Clear the keys we took (non-atomic with respect to newly inserted)
            for k in &keys {
                DIRTY_SET.remove(k);
            }

            // process in chunks to avoid huge IN lists
            const CHUNK_KEYS: usize = 200;
            for chunk in keys.chunks(CHUNK_KEYS) {
                // Build query using IN (?,?,...) on the concatenated key
                // Note: We're using (symbol || ':' || chain_code || ':' || token_address) to match the stored key
                let mut query = String::from(
                    "SELECT id, symbol, chain_code, token_address, balance, decimals FROM api_assets WHERE (symbol || ':' || chain_code || ':' || token_address) IN (",
                );
                query.push_str(&chunk.iter().map(|_| "?").collect::<Vec<_>>().join(","));
                query.push(')');

                let mut q = sqlx::query(&query);
                for k in chunk {
                    q = q.bind(k);
                }

                let rows = match q.fetch_all(db.as_ref()).await {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!("batch query error: {}", e);
                        continue;
                    }
                };

                let mut assets: Vec<AssetEntry> = Vec::with_capacity(rows.len());
                for row in rows {
                    let id: i64 = row.get("id");
                    let symbol: String = row.get("symbol");
                    let chain_code: String = row.get("chain_code");
                    let token_address: String = row.get("token_address");
                    // balance assumed stored as TEXT in DB for compatibility; parse to f64
                    let balance_raw: Option<String> = row.get("balance");
                    let balance = balance_raw.and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                    let decimals: i32 = row.get("decimals");

                    assets.push(AssetEntry {
                        id,
                        symbol,
                        chain_code,
                        token_address,
                        balance,
                        decimals,
                    });
                }

                // parallel compute
                assets.par_iter().for_each(|asset| {
                    let key = make_key(&asset.symbol, &asset.chain_code, &asset.token_address);
                    if let Some(p) = PRICE_CACHE.get(&key) {
                        let denom = 10f64.powi(asset.decimals);
                        let value = (asset.balance / denom) * p.price;
                        ASSET_VALUE_CACHE.insert(asset.id, value);
                    } else {
                        ASSET_VALUE_CACHE.insert(asset.id, 0.0);
                    }
                });
            }

            // recompute total (simple reduction)
            let total: f64 = ASSET_VALUE_CACHE.iter().map(|kv| *kv.value()).sum();
            if let Ok(mut t) = TOTAL_USDT.try_write() {
                *t = total;
            } else {
                let total_clone = total;
                tokio::spawn(async move {
                    let mut guard = TOTAL_USDT.write().await;
                    *guard = total_clone;
                });
            }
        }
    });
}

/// Force full refresh: runs a full join query and repopulates ASSET_VALUE_CACHE (accurate, but heavy)
pub async fn force_refresh_all_assets(db: Arc<SqlitePool>) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT a.id, a.symbol, a.chain_code, a.token_address, a.balance, a.decimals, c.price_real
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

    // convert rows into vector then parallel compute
    let mut assets: Vec<(i64, AssetEntry, f64)> = Vec::with_capacity(rows.len());
    for row in rows {
        let id: i64 = row.get("id");
        let symbol: String = row.get("symbol");
        let chain_code: String = row.get("chain_code");
        let token_address: String = row.get("token_address");
        let balance_raw: Option<String> = row.get("balance");
        let balance = balance_raw.and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
        let decimals: i32 = row.get("decimals");
        let price_real: Option<f64> = row.try_get("price_real").ok();
        let price = price_real.unwrap_or(0.0);

        let asset = AssetEntry { id, symbol, chain_code, token_address, balance, decimals };
        assets.push((id, asset, price));
    }

    assets.par_iter().for_each(|(id, asset, price)| {
        let denom = 10f64.powi(asset.decimals);
        let value = (asset.balance / denom) * *price;
        ASSET_VALUE_CACHE.insert(*id, value);
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
    let g = TOTAL_USDT.read().await;
    *g
}

/// Get a page of asset snapshot (id and usdt_value)
/// This implementation fetches asset ids from DB by page, then maps to cached values.
pub async fn get_asset_snapshot_page(
    db: Arc<SqlitePool>,
    page: usize,
    page_size: usize,
) -> Result<Vec<(i64, f64)>, sqlx::Error> {
    let offset = ((page.saturating_sub(1) * page_size) as i64).max(0);
    let rows = sqlx::query("SELECT id FROM api_assets ORDER BY id LIMIT ? OFFSET ?")
        .bind(page_size as i64)
        .bind(offset)
        .fetch_all(db.as_ref())
        .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: i64 = row.get("id");
        let value = ASSET_VALUE_CACHE.get(&id).map(|v| *v.value()).unwrap_or(0.0);
        out.push((id, value));
    }
    Ok(out)
}
