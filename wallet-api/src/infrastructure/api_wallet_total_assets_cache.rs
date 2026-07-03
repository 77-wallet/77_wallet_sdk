use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::time::Instant;
use tokio::time::Duration;

use crate::response_vo::standard_wallet::account::BalanceInfo;

#[derive(Clone)]
struct CachedWalletTotalAssets {
    value: BalanceInfo,
    updated_at: Instant,
}

static WALLET_TOTAL_ASSETS_CACHE: Lazy<DashMap<String, CachedWalletTotalAssets>> =
    Lazy::new(DashMap::new);
static WALLET_TOTAL_ASSETS_REFRESHING: Lazy<DashMap<String, ()>> = Lazy::new(DashMap::new);

pub(crate) fn wallet_total_assets_key(
    wallet_address: &str,
    account_id: Option<u32>,
    chain_code: Option<&str>,
) -> String {
    let account_part = account_id.map(|v| v.to_string()).unwrap_or_else(|| "none".to_string());
    let chain_part = chain_code.unwrap_or("none");
    format!("wallet={wallet_address};account_id={account_part};chain_code={chain_part}")
}

pub(crate) fn get_fresh(key: &str, max_age: Duration) -> Option<BalanceInfo> {
    WALLET_TOTAL_ASSETS_CACHE.get(key).and_then(|entry| {
        if entry.updated_at.elapsed() <= max_age { Some(entry.value.clone()) } else { None }
    })
}

pub(crate) fn get_stale(key: &str) -> Option<BalanceInfo> {
    WALLET_TOTAL_ASSETS_CACHE.get(key).map(|entry| entry.value.clone())
}

pub(crate) fn set(key: &str, value: &BalanceInfo) {
    WALLET_TOTAL_ASSETS_CACHE.insert(
        key.to_string(),
        CachedWalletTotalAssets { value: value.clone(), updated_at: Instant::now() },
    );
}

pub(crate) fn mark_wallet_stale(wallet_address: &str, fresh_ttl: Duration) -> Vec<String> {
    let keys: Vec<String> = WALLET_TOTAL_ASSETS_CACHE
        .iter()
        .filter(|entry| entry.key().starts_with(&format!("wallet={wallet_address};")))
        .map(|entry| entry.key().clone())
        .collect();

    for key in &keys {
        mark_key_stale(key, fresh_ttl);
    }

    keys
}

pub(crate) fn mark_key_stale(key: &str, fresh_ttl: Duration) {
    if let Some(mut entry) = WALLET_TOTAL_ASSETS_CACHE.get_mut(key) {
        let stale_at = Instant::now()
            .checked_sub(fresh_ttl + Duration::from_millis(1))
            .unwrap_or_else(Instant::now);
        entry.updated_at = stale_at;
    }
}

pub(crate) fn schedule_refresh(
    ctx: &'static crate::context::Context,
    wallet_address: String,
) -> bool {
    let cache_key = wallet_total_assets_key(&wallet_address, None, None);
    if WALLET_TOTAL_ASSETS_REFRESHING.insert(cache_key.clone(), ()).is_some() {
        tracing::debug!(
            cache_key = %cache_key,
            wallet_address = %wallet_address,
            "api wallet total-assets refresh already scheduled"
        );
        return false;
    }

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = crate::domain::api_wallet::assets::ApiAssetsDomain::get_api_wallet_assets(
            ctx,
            Some(&wallet_address),
            None,
            None,
        )
        .await;
        match result {
            Ok(_) => {
                tracing::debug!(
                    cache_key = %cache_key,
                    wallet_address = %wallet_address,
                    "api wallet total-assets cache refreshed after balance sync"
                );
            }
            Err(err) => {
                tracing::warn!(
                    cache_key = %cache_key,
                    wallet_address = %wallet_address,
                    err = %err,
                    "api wallet total-assets cache refresh failed after balance sync"
                );
            }
        }
        WALLET_TOTAL_ASSETS_REFRESHING.remove(&cache_key);
    });

    true
}
