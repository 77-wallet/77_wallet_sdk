use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const DEFAULT_RECOVER_COOLDOWN_MS: u64 = 5_000;
const MIN_RECOVER_COOLDOWN_MS: u64 = 1_000;
const MAX_RECOVER_COOLDOWN_MS: u64 = 30_000;
const BREAKER_WARN_COOLDOWN: Duration = Duration::from_secs(10);
const MAX_RECOVER_COOLDOWN_MAP_SIZE: usize = 100_000;
const INTENT_REPORT_WINDOW_SECS: u64 = 60;

static RECOVER_LAST_DISPATCH_TS: Lazy<DashMap<String, Instant>> = Lazy::new(DashMap::new);
static BREAKER_WARN_LAST_LOG_TS: Lazy<DashMap<String, Instant>> = Lazy::new(DashMap::new);
static BROADCAST_INTENT_DISPATCH_COUNT: AtomicU64 = AtomicU64::new(0);
static RECOVER_INTENT_DISPATCH_COUNT: AtomicU64 = AtomicU64::new(0);
static LAST_INTENT_REPORT_TS: AtomicU64 = AtomicU64::new(0);

fn parse_usize_env(raw: Option<&str>, default: usize, min: usize, max: usize) -> usize {
    let parsed = raw.and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(default);
    parsed.clamp(min, max)
}

fn parse_u64_env(raw: Option<&str>, default: u64, min: u64, max: u64) -> u64 {
    let parsed = raw.and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(default);
    parsed.clamp(min, max)
}

pub(crate) fn read_usize_env(name: &str, default: usize, min: usize, max: usize) -> usize {
    parse_usize_env(std::env::var(name).ok().as_deref(), default, min, max)
}

pub(crate) fn read_u64_env(name: &str, default: u64, min: u64, max: u64) -> u64 {
    parse_u64_env(std::env::var(name).ok().as_deref(), default, min, max)
}

pub(crate) fn recover_cooldown() -> Duration {
    Duration::from_millis(read_u64_env(
        "SHADOW_RECOVER_COOLDOWN_MS",
        DEFAULT_RECOVER_COOLDOWN_MS,
        MIN_RECOVER_COOLDOWN_MS,
        MAX_RECOVER_COOLDOWN_MS,
    ))
}

pub(crate) fn allow_recover_dispatch(key: &str) -> bool {
    let now = Instant::now();
    let cooldown = recover_cooldown();

    if let Some(last_ts) = RECOVER_LAST_DISPATCH_TS.get(key) {
        if now.duration_since(*last_ts) < cooldown {
            return false;
        }
    }

    RECOVER_LAST_DISPATCH_TS.insert(key.to_string(), now);

    if RECOVER_LAST_DISPATCH_TS.len() > MAX_RECOVER_COOLDOWN_MAP_SIZE {
        let ttl_ms = (cooldown.as_millis() as u64).saturating_mul(8);
        let ttl = Duration::from_millis(ttl_ms);
        RECOVER_LAST_DISPATCH_TS.retain(|_, ts| now.duration_since(*ts) < ttl);
    }

    true
}

pub(crate) async fn breaker_open_for_chain_code(
    ctx: &crate::context::Context,
    chain_code: &str,
) -> Option<(String, Duration)> {
    crate::infrastructure::chain_rpc_guard::breaker_open_for_chain_code_with_ctx(ctx, chain_code)
        .await
}

pub(crate) fn should_emit_breaker_warn(key: &str) -> bool {
    let now = Instant::now();
    if let Some(last_ts) = BREAKER_WARN_LAST_LOG_TS.get(key) {
        if now.duration_since(*last_ts) < BREAKER_WARN_COOLDOWN {
            return false;
        }
    }

    BREAKER_WARN_LAST_LOG_TS.insert(key.to_string(), now);
    true
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::from_secs(0)).as_secs()
}

pub(crate) fn record_chain_intent_dispatch(intent_kind: &str) {
    match intent_kind {
        "broadcast" => {
            BROADCAST_INTENT_DISPATCH_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        "recover" => {
            RECOVER_INTENT_DISPATCH_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        _ => return,
    }

    let now = now_secs();
    let last = LAST_INTENT_REPORT_TS.load(Ordering::Relaxed);

    if last == 0 {
        let _ =
            LAST_INTENT_REPORT_TS.compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
        return;
    }

    if now.saturating_sub(last) < INTENT_REPORT_WINDOW_SECS {
        return;
    }

    if LAST_INTENT_REPORT_TS
        .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        let broadcast = BROADCAST_INTENT_DISPATCH_COUNT.swap(0, Ordering::Relaxed);
        let recover = RECOVER_INTENT_DISPATCH_COUNT.swap(0, Ordering::Relaxed);
        tracing::info!(
            broadcast = broadcast,
            recover = recover,
            "shadow intent dispatch count in last minute"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_u64_env, parse_usize_env};

    #[test]
    fn parse_usize_env_uses_default_when_missing_or_invalid() {
        assert_eq!(parse_usize_env(None, 24, 4, 100), 24);
        assert_eq!(parse_usize_env(Some("bad"), 24, 4, 100), 24);
    }

    #[test]
    fn parse_usize_env_clamps_range() {
        assert_eq!(parse_usize_env(Some("1"), 24, 4, 100), 4);
        assert_eq!(parse_usize_env(Some("300"), 24, 4, 100), 100);
        assert_eq!(parse_usize_env(Some("32"), 24, 4, 100), 32);
    }

    #[test]
    fn parse_u64_env_uses_default_when_missing_or_invalid() {
        assert_eq!(parse_u64_env(None, 5000, 1000, 30000), 5000);
        assert_eq!(parse_u64_env(Some("bad"), 5000, 1000, 30000), 5000);
    }

    #[test]
    fn parse_u64_env_clamps_range() {
        assert_eq!(parse_u64_env(Some("999"), 5000, 1000, 30000), 1000);
        assert_eq!(parse_u64_env(Some("30001"), 5000, 1000, 30000), 30000);
        assert_eq!(parse_u64_env(Some("8000"), 5000, 1000, 30000), 8000);
    }
}
