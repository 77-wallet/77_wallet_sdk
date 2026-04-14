use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::{
    future::Future,
    sync::{Arc, Weak},
    time::Instant,
};
use tokio::time::Duration;

use crate::{config::runtime_defaults, response_vo::standard_wallet::account::BalanceInfo};

use super::{ApiAccountRepo, ApiAssetsDomain, singleflight};

#[derive(Clone)]
struct CachedWalletTotalAssets {
    // 保存最近一次成功聚合结果，用于短 TTL 命中和失败时 stale 返回。
    value: BalanceInfo,
    updated_at: Instant,
}

static WALLET_TOTAL_ASSETS_V3_LOCKS: Lazy<DashMap<String, Weak<tokio::sync::Mutex<()>>>> =
    Lazy::new(DashMap::new);

// 与锁分开存放，避免锁生命周期影响缓存命中。
static WALLET_TOTAL_ASSETS_CACHE: Lazy<DashMap<String, CachedWalletTotalAssets>> =
    Lazy::new(DashMap::new);

pub(super) fn wallet_total_assets_v3_lock_key(
    wallet_address: &str,
    account_id: Option<u32>,
    chain_code: Option<&str>,
) -> String {
    let account_part = account_id.map(|v| v.to_string()).unwrap_or_else(|| "none".to_string());
    let chain_part = chain_code.unwrap_or("none");
    format!("wallet={wallet_address};account_id={account_part};chain_code={chain_part}")
}

fn wallet_total_assets_query_key(
    wallet_address: Option<&str>,
    account_id: Option<u32>,
    chain_code: Option<&str>,
) -> String {
    // 允许 wallet_address 为空，保持“全局查询”也能走统一去重/缓存键格式。
    let wallet_part = wallet_address.unwrap_or("none");
    wallet_total_assets_v3_lock_key(wallet_part, account_id, chain_code)
}

pub(super) fn wallet_total_assets_query_lock(key: &str) -> Arc<tokio::sync::Mutex<()>> {
    if let Some(entry) = WALLET_TOTAL_ASSETS_V3_LOCKS.get(key) {
        if let Some(lock) = entry.value().upgrade() {
            return lock;
        }
    }
    // 使用 Weak 存储避免 key 常驻导致锁对象无法释放。
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    WALLET_TOTAL_ASSETS_V3_LOCKS.insert(key.to_string(), Arc::downgrade(&lock));
    lock
}

fn get_cached_wallet_total_assets(key: &str, max_age: Duration) -> Option<BalanceInfo> {
    // 仅在 age 窗口内返回，调用侧可分别传 fresh TTL 或 stale TTL。
    WALLET_TOTAL_ASSETS_CACHE.get(key).and_then(|entry| {
        if entry.updated_at.elapsed() <= max_age { Some(entry.value.clone()) } else { None }
    })
}

fn set_cached_wallet_total_assets(key: &str, value: &BalanceInfo) {
    WALLET_TOTAL_ASSETS_CACHE.insert(
        key.to_string(),
        CachedWalletTotalAssets { value: value.clone(), updated_at: Instant::now() },
    );
}

fn is_db_pool_timeout_error(err: &str) -> bool {
    err.contains("pool timed out while waiting for an open connection")
}

fn is_v3_timeout_or_pool_timeout(err: &crate::error::service::ServiceError) -> bool {
    matches!(err, crate::error::service::ServiceError::Timeout)
        || is_db_pool_timeout_error(&err.to_string())
}

fn log_db_pool_timeout_metric(err: &str) {
    if is_db_pool_timeout_error(err) {
        tracing::warn!(metric = "db_pool_timeout_count", err = %err, "db pool timeout");
    }
}

// 同 key 聚合请求的统一入口：
// 先命中新鲜缓存，再通过 per-key lock 合并并发查询；查询失败时优先返回 stale 缓存止血。
async fn load_total_assets_with_cache<F, Fut>(
    cache_key: &str,
    fresh_ttl: Duration,
    stale_grace: Duration,
    query_fn: F,
) -> Result<BalanceInfo, crate::error::service::ServiceError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<BalanceInfo, crate::error::service::ServiceError>>,
{
    if let Some(cached) = get_cached_wallet_total_assets(cache_key, fresh_ttl) {
        tracing::info!(
            metric = "api_assets_cache_hit",
            cache_key = %cache_key,
            cache_type = "fresh",
            "wallet assets cache hit"
        );
        return Ok(cached);
    }

    let wait_start = Instant::now();
    let result = singleflight::call_balance_info(cache_key, || async move {
        if let Some(cached) = get_cached_wallet_total_assets(cache_key, fresh_ttl) {
            tracing::info!(
                metric = "api_assets_dedup_hit",
                cache_key = %cache_key,
                "wallet assets dedup hit"
            );
            return Ok(cached);
        }

        match query_fn().await.map_err(|e| e.to_string()) {
            Ok(balance) => {
                set_cached_wallet_total_assets(cache_key, &balance);
                Ok(balance)
            }
            Err(err) => {
                log_db_pool_timeout_metric(&err.to_string());
                let stale_ttl = fresh_ttl + stale_grace;
                if let Some(stale) = get_cached_wallet_total_assets(cache_key, stale_ttl) {
                    tracing::warn!(
                        metric = "api_assets_stale_return",
                        cache_key = %cache_key,
                        err = %err,
                        "wallet assets query failed, return stale cache"
                    );
                    return Ok(stale);
                }
                Err(err)
            }
        }
    })
    .await;
    let wait_elapsed_ms = wait_start.elapsed().as_millis();
    if wait_elapsed_ms > 0 {
        tracing::info!(
            metric = "api_assets_singleflight_wait_ms",
            cache_key = %cache_key,
            wait_elapsed_ms,
            "wallet assets single-flight wait finished"
        );
    }

    result.map_err(crate::error::service::ServiceError::Parameter)
}

impl ApiAssetsDomain {
    pub async fn get_api_wallet_assets(
        wallet_address: Option<&str>,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<BalanceInfo, crate::error::service::ServiceError> {
        // 统一从集中默认值读取，避免缓存/阈值参数散落在 service 与 domain 间。
        let defaults = runtime_defaults::api_assets();
        let cache_key = wallet_total_assets_query_key(wallet_address, account_id, chain_code);
        let fresh_ttl = defaults.total_cache_ttl;
        let stale_grace = defaults.stale_grace;
        let allow_v2_fallback_large = defaults.allow_v2_fallback_large_wallet;
        let cache_key_for_query = cache_key.clone();
        let small_wallet_address_threshold = defaults.small_wallet_address_threshold;

        load_total_assets_with_cache(&cache_key, fresh_ttl, stale_grace, || async move {
            if let Some(wallet_address) = wallet_address {
                let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
                let count_start = std::time::Instant::now();
                let address_count = ApiAccountRepo::count_by_wallet_address(
                    &pool,
                    wallet_address,
                    account_id,
                    chain_code,
                )
                .await
                .unwrap_or(small_wallet_address_threshold + 1);
                tracing::info!(
                    metric = "api_assets_wallet_account_count_ms",
                    cache_key = %cache_key_for_query,
                    wallet_address = %wallet_address,
                    account_id = ?account_id,
                    chain_code = ?chain_code,
                    address_count,
                    elapsed_ms = count_start.elapsed().as_millis(),
                    "wallet total assets account count finished"
                );

                if address_count <= small_wallet_address_threshold {
                    tracing::info!(
                        metric = "api_assets_total_path",
                        cache_key = %cache_key_for_query,
                        wallet_address = %wallet_address,
                        address_count,
                        path = "v2",
                        "wallet total assets using v2 path"
                    );
                    return Self::get_api_wallet_assets_v2(Some(wallet_address), account_id, chain_code)
                        .await;
                }

                // 这里已经持有 per-key query lock，避免再走 v3 内部同 key 锁导致自锁超时。
                tracing::info!(
                    metric = "api_assets_total_path",
                    cache_key = %cache_key_for_query,
                    wallet_address = %wallet_address,
                    address_count,
                    path = "v3",
                    "wallet total assets using v3 path"
                );
                match Self::get_api_wallet_assets_v3_unlocked(wallet_address, account_id, chain_code)
                    .await
                {
                    Ok(res) => Ok(res),
                    Err(e) => {
                        // 大钱包 v3 失败最常见是超时/连接池耗尽，单独打点便于观察止血效果。
                        if is_v3_timeout_or_pool_timeout(&e) {
                            tracing::warn!(
                                metric = "api_assets_v3_timeout",
                                cache_key = %cache_key_for_query,
                                err = %e,
                                address_count,
                                "wallet total assets v3 timeout or pool-timeout"
                            );
                        }

                        if allow_v2_fallback_large {
                            tracing::warn!(
                                err = ?e,
                                address_count,
                                cache_key = %cache_key_for_query,
                                "get_api_wallet_assets: v3 failed, fallback to v2 (enabled)"
                            );
                            Self::get_api_wallet_assets_v2(Some(wallet_address), account_id, chain_code)
                                .await
                        } else {
                            // 默认禁止回退 v2：避免在高压场景回到重 SQL 路径，造成二次放大。
                            tracing::warn!(
                                metric = "api_assets_v2_fallback_blocked",
                                err = %e,
                                address_count,
                                cache_key = %cache_key_for_query,
                                "get_api_wallet_assets: v3 failed, fallback blocked for large wallet"
                            );
                            Err(e)
                        }
                    }
                }
            } else {
                Self::get_api_wallet_assets_v2(None, account_id, chain_code).await
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::{SystemTime, UNIX_EPOCH},
    };

    use tokio::time::{Duration, sleep};

    use crate::{error::service::ServiceError, response_vo::standard_wallet::account::BalanceInfo};

    use super::{load_total_assets_with_cache, set_cached_wallet_total_assets};

    fn unique_key(prefix: &str) -> String {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        format!("{prefix}-{ts}")
    }

    #[tokio::test]
    async fn total_assets_dedup_allows_single_real_query() {
        let key = unique_key("assets-dedup");
        let hit_count = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();

        for _ in 0..50 {
            let hit_count = hit_count.clone();
            let key = key.clone();
            tasks.push(tokio::spawn(async move {
                load_total_assets_with_cache(
                    &key,
                    Duration::from_secs(5),
                    Duration::from_secs(5),
                    || async move {
                        hit_count.fetch_add(1, Ordering::SeqCst);
                        sleep(Duration::from_millis(30)).await;
                        Ok(BalanceInfo {
                            amount: 1.0,
                            currency: "USD".to_string(),
                            unit_price: None,
                            fiat_value: Some(1.0),
                        })
                    },
                )
                .await
            }));
        }

        for t in tasks {
            let res = t.await.expect("join ok");
            assert!(res.is_ok());
        }

        assert_eq!(hit_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn total_assets_query_returns_stale_when_refresh_fails() {
        let key = unique_key("assets-stale");
        set_cached_wallet_total_assets(
            &key,
            &BalanceInfo {
                amount: 7.0,
                currency: "USD".to_string(),
                unit_price: None,
                fiat_value: Some(7.0),
            },
        );

        let res = load_total_assets_with_cache(
            &key,
            Duration::from_millis(0),
            Duration::from_secs(30),
            || async { Err(ServiceError::Timeout) },
        )
        .await
        .expect("should return stale cache");

        assert_eq!(res.amount, 7.0);
    }
}
