use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Chain RPC guard: shared concurrency limiter + per-endpoint circuit breaker.
///
/// Design goals:
/// - Multi-chain friendly: do NOT hardcode "TRON-only" semantics into module names or call-sites.
/// - Opt-in by RPC endpoint host allowlist, so production mainnet is not impacted unless configured.
/// - Best-effort: guard failures MUST NOT block business flows.
///
/// Configuration (env):
/// - `CHAIN_RPC_GUARD_HOSTS`: comma-separated hosts to guard (default: `api.nileex.io`)
/// - `CHAIN_RPC_GUARD_MAX_CONCURRENCY`: guarded-endpoint global concurrency (default: 6)
const DEFAULT_GUARDED_HOSTS: &str = "api.nileex.io";
const DEFAULT_MAX_CONCURRENCY: usize = 6;
const CHAIN_HOST_CACHE_TTL: Duration = Duration::from_secs(60);
const TRANSIENT_503_REPORT_WINDOW_MS: u64 = 60_000;

static GUARDED_HOSTS: Lazy<HashSet<String>> = Lazy::new(|| {
    let raw =
        std::env::var("CHAIN_RPC_GUARD_HOSTS").unwrap_or_else(|_| DEFAULT_GUARDED_HOSTS.into());
    raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).map(|s| s.to_string()).collect()
});

fn max_concurrency() -> usize {
    std::env::var("CHAIN_RPC_GUARD_MAX_CONCURRENCY")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_MAX_CONCURRENCY)
}

static CHAIN_RPC_SEM: Lazy<Arc<Semaphore>> =
    Lazy::new(|| Arc::new(Semaphore::new(max_concurrency())));
static TRANSIENT_503_COUNT: AtomicU64 = AtomicU64::new(0);
static LAST_TRANSIENT_503_REPORT_MS: AtomicU64 = AtomicU64::new(0);

pub fn shared_chain_rpc_semaphore() -> Arc<Semaphore> {
    CHAIN_RPC_SEM.clone()
}

#[derive(Default)]
pub struct RpcCircuitBreaker {
    open_until_ms: AtomicU64,
    consecutive_failures: AtomicU32,
    last_failure_ms: AtomicU64,
    last_reported_open_until_ms: AtomicU64,
}

impl RpcCircuitBreaker {
    fn now_ms() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::from_secs(0)).as_millis()
            as u64
    }

    pub fn is_open(&self) -> bool {
        let now = Self::now_ms();
        now < self.open_until_ms.load(Ordering::Relaxed)
    }

    pub fn remaining(&self) -> Option<Duration> {
        let now = Self::now_ms();
        let until = self.open_until_ms.load(Ordering::Relaxed);
        if now >= until { None } else { Some(Duration::from_millis(until - now)) }
    }

    pub fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.last_failure_ms.store(0, Ordering::Relaxed);
        let last_reported = self.last_reported_open_until_ms.swap(0, Ordering::Relaxed);
        if last_reported != 0 {
            tracing::info!("chain rpc circuit breaker closed");
        }
    }

    pub fn record_failure(&self) {
        let now = Self::now_ms();
        let last = self.last_failure_ms.swap(now, Ordering::Relaxed);

        let mut streak = if last == 0 || now.saturating_sub(last) > 10_000 {
            1
        } else {
            self.consecutive_failures.load(Ordering::Relaxed).saturating_add(1)
        };
        if streak > 1000 {
            streak = 1000;
        }
        self.consecutive_failures.store(streak, Ordering::Relaxed);

        if streak >= 5 {
            let open_for = Duration::from_secs(30);
            let new_until = now.saturating_add(open_for.as_millis() as u64);
            self.open_until_ms.store(new_until, Ordering::Relaxed);

            let prev_reported = self.last_reported_open_until_ms.swap(new_until, Ordering::Relaxed);
            if prev_reported == 0 || prev_reported < now {
                tracing::warn!(streak = streak, open_for = ?open_for, "chain rpc circuit breaker opened");
            }
        }
    }
}

static BREAKERS: Lazy<DashMap<String, Arc<RpcCircuitBreaker>>> = Lazy::new(DashMap::new);

fn breaker_for_host(host: &str) -> Arc<RpcCircuitBreaker> {
    BREAKERS
        .entry(host.to_string())
        .or_insert_with(|| Arc::new(RpcCircuitBreaker::default()))
        .clone()
}

#[derive(Clone)]
struct ChainHostCacheEntry {
    host: Option<String>,
    fetched_at_ms: u64,
}

static CHAIN_HOST_CACHE: Lazy<DashMap<String, ChainHostCacheEntry>> = Lazy::new(DashMap::new);

fn host_from_rpc_url(rpc_url: &str) -> Option<String> {
    // Best-effort parsing:
    // - strip scheme if any
    // - take authority until first '/'
    // - strip port if present
    let without_scheme = rpc_url
        .strip_prefix("https://")
        .or_else(|| rpc_url.strip_prefix("http://"))
        .unwrap_or(rpc_url);
    let authority = without_scheme.split('/').next().unwrap_or(without_scheme);
    let host = authority.split('@').last().unwrap_or(authority);
    let host = host.strip_prefix('[').unwrap_or(host);
    let host = host.split(']').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    let host = host.trim();
    if host.is_empty() { None } else { Some(host.to_string()) }
}

fn is_guarded_host(host: &str) -> bool {
    GUARDED_HOSTS.contains(host)
}

async fn fetch_chain_rpc_host(chain_code: &str) -> Option<String> {
    let ctx = crate::context::CONTEXT.get()?;
    let core_pool = ctx.core_pool().ok()?;
    let api_pool = ctx.api_wallet_pool().ok()?;

    let ensurer = crate::infrastructure::chain_node::chain_node_ensurer::ChainNodeEnsurer::new(
        core_pool, api_pool,
    );
    let chain_with_node = ensurer.ensure_and_get_api_chain_with_node(chain_code).await.ok()?;
    host_from_rpc_url(&chain_with_node.rpc_url)
}

async fn guarded_host_for_chain_code(chain_code: &str) -> Option<String> {
    let now = RpcCircuitBreaker::now_ms();
    if let Some(entry) = CHAIN_HOST_CACHE.get(chain_code) {
        if now.saturating_sub(entry.fetched_at_ms) <= CHAIN_HOST_CACHE_TTL.as_millis() as u64 {
            return entry.host.clone().filter(|h| is_guarded_host(h));
        }
    }

    let host = fetch_chain_rpc_host(chain_code).await;
    CHAIN_HOST_CACHE.insert(
        chain_code.to_string(),
        ChainHostCacheEntry { host: host.clone(), fetched_at_ms: now },
    );
    host.filter(|h| is_guarded_host(h))
}

pub async fn breaker_open_for_chain_code(chain_code: &str) -> Option<(String, Duration)> {
    let host = guarded_host_for_chain_code(chain_code).await?;
    let breaker = breaker_for_host(&host);
    if breaker.is_open() {
        let remaining = breaker.remaining().unwrap_or(Duration::from_secs(0));
        Some((host, remaining))
    } else {
        None
    }
}

pub async fn acquire_if_guarded(chain_code: &str) -> Option<OwnedSemaphorePermit> {
    let _host = guarded_host_for_chain_code(chain_code).await?;
    let sem = shared_chain_rpc_semaphore();
    match sem.acquire_owned().await {
        Ok(p) => Some(p),
        Err(_) => {
            tracing::error!("chain rpc semaphore closed; skip guard acquire");
            None
        }
    }
}

pub async fn record_success_for_chain_code(chain_code: &str) {
    let Some(host) = guarded_host_for_chain_code(chain_code).await else { return };
    breaker_for_host(&host).record_success();
}

fn maybe_report_transient_503_count(now_ms: u64) {
    let last = LAST_TRANSIENT_503_REPORT_MS.load(Ordering::Relaxed);
    if last == 0 {
        let _ = LAST_TRANSIENT_503_REPORT_MS.compare_exchange(
            0,
            now_ms,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
        return;
    }

    if now_ms.saturating_sub(last) < TRANSIENT_503_REPORT_WINDOW_MS {
        return;
    }

    if LAST_TRANSIENT_503_REPORT_MS
        .compare_exchange(last, now_ms, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        let count = TRANSIENT_503_COUNT.swap(0, Ordering::Relaxed);
        tracing::info!(count = count, "chain rpc code=503 count in last minute");
    }
}

pub fn record_transient_failure_from_error<E: std::fmt::Display>(err: &E) {
    let s = err.to_string();
    if s.contains("code=503") {
        TRANSIENT_503_COUNT.fetch_add(1, Ordering::Relaxed);
        maybe_report_transient_503_count(RpcCircuitBreaker::now_ms());
    }

    if !is_transient_chain_rpc_error_message(&s) {
        return;
    }
    let Some(host) = guarded_host_from_error_message(&s) else { return };
    if !is_guarded_host(&host) {
        return;
    }
    breaker_for_host(&host).record_failure();
}

fn guarded_host_from_error_message(msg: &str) -> Option<String> {
    GUARDED_HOSTS.iter().find(|h| msg.contains(h.as_str())).cloned()
}

pub fn is_transient_chain_rpc_error_message(msg: &str) -> bool {
    // Transient patterns seen under pressure:
    // - HTTP 503 + HTML error pages
    // - Empty JSON / partial bodies causing deserialization errors
    msg.contains("code=503")
        || msg.contains("<!DOCTYPE html>")
        || msg.contains("missing field `Error`")
        || msg.contains("value = {}")
        || msg.contains("all response:{}")
}
