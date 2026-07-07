use crate::{context::Context, error::service::ServiceError};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::{
    collections::HashSet,
    future::Future,
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
/// - `CHAIN_RPC_GUARD_ACQUIRE_TIMEOUT_MS`: max wait for a guarded permit (default: 30000)
/// - `CHAIN_RPC_GUARD_RPC_TIMEOUT_MS`: max guarded RPC await time (default: 60000)
const DEFAULT_GUARDED_HOSTS: &str = "api.nileex.io";
const DEFAULT_MAX_CONCURRENCY: usize = 6;
const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_RPC_TIMEOUT: Duration = Duration::from_secs(60);
const CHAIN_HOST_CACHE_TTL: Duration = Duration::from_secs(60);
const TRANSIENT_503_REPORT_WINDOW_MS: u64 = 60_000;
const HTTP_STATUS_SERVICE_UNAVAILABLE: u16 = 503;

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

fn acquire_timeout() -> Duration {
    std::env::var("CHAIN_RPC_GUARD_ACQUIRE_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_ACQUIRE_TIMEOUT)
}

fn rpc_timeout() -> Duration {
    std::env::var("CHAIN_RPC_GUARD_RPC_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_RPC_TIMEOUT)
}

static CHAIN_RPC_SEM: Lazy<Arc<Semaphore>> =
    Lazy::new(|| Arc::new(Semaphore::new(max_concurrency())));
static TRANSIENT_503_COUNT: AtomicU64 = AtomicU64::new(0);
static LAST_TRANSIENT_503_REPORT_MS: AtomicU64 = AtomicU64::new(0);

pub fn shared_chain_rpc_semaphore() -> Arc<Semaphore> {
    CHAIN_RPC_SEM.clone()
}

pub struct ChainRpcGuardPermit {
    host: String,
    _permit: OwnedSemaphorePermit,
    acquired_at: std::time::Instant,
}

impl ChainRpcGuardPermit {
    fn new(host: String, permit: OwnedSemaphorePermit) -> Self {
        Self { host, _permit: permit, acquired_at: std::time::Instant::now() }
    }
}

impl Drop for ChainRpcGuardPermit {
    fn drop(&mut self) {
        tracing::debug!(
            host = %self.host,
            held = ?self.acquired_at.elapsed(),
            "released chain rpc guard permit"
        );
    }
}

pub enum ChainRpcGuardAcquire {
    NotGuarded,
    Acquired { host: String, permit: ChainRpcGuardPermit, waited: Duration },
    TimedOut { host: String, timeout: Duration },
    Closed { host: String },
}

pub enum ChainRpcGuardAttempt {
    Proceed(Option<ChainRpcGuardPermit>),
    Skip,
}

#[derive(Default)]
pub struct RpcCircuitBreaker {
    open_until_ms: AtomicU64,
    consecutive_failures: AtomicU32,
    last_failure_ms: AtomicU64,
    last_reported_open_until_ms: AtomicU64,
    last_error_code: AtomicU32,
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

    pub fn last_error_code(&self) -> Option<u16> {
        let code = self.last_error_code.load(Ordering::Relaxed);
        if code == 0 { None } else { Some(code as u16) }
    }

    pub fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.last_failure_ms.store(0, Ordering::Relaxed);
        self.last_error_code.store(0, Ordering::Relaxed);
        // 成功后立即清空 open_until，避免熔断窗口继续阻塞后续请求。
        self.open_until_ms.store(0, Ordering::Relaxed);
        let last_reported = self.last_reported_open_until_ms.swap(0, Ordering::Relaxed);
        if last_reported != 0 {
            tracing::info!("chain rpc circuit breaker closed");
        }
    }

    pub fn record_failure(&self, error_code: Option<u16>) {
        if let Some(code) = error_code {
            self.last_error_code.store(code as u32, Ordering::Relaxed);
        }
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

async fn fetch_chain_rpc_host(chain_code: &str, ctx: &Context) -> Option<String> {
    let core_pool = ctx.core_pool().ok()?;
    let api_pool = ctx.api_wallet_pool().ok()?;

    let ensurer = crate::infrastructure::chain_node::chain_node_ensurer::ChainNodeEnsurer::new(
        core_pool, api_pool,
    );
    let chain_with_node = ensurer.ensure_and_get_api_chain_with_node(chain_code).await.ok()?;
    host_from_rpc_url(&chain_with_node.rpc_url)
}

async fn guarded_host_for_chain_code(chain_code: &str, ctx: &Context) -> Option<String> {
    let now = RpcCircuitBreaker::now_ms();
    if let Some(entry) = CHAIN_HOST_CACHE.get(chain_code) {
        if now.saturating_sub(entry.fetched_at_ms) <= CHAIN_HOST_CACHE_TTL.as_millis() as u64 {
            return entry.host.clone().filter(|h| is_guarded_host(h));
        }
    }

    let host = fetch_chain_rpc_host(chain_code, ctx).await;
    CHAIN_HOST_CACHE.insert(
        chain_code.to_string(),
        ChainHostCacheEntry { host: host.clone(), fetched_at_ms: now },
    );
    host.filter(|h| is_guarded_host(h))
}

pub async fn breaker_open_for_chain_code_with_ctx(
    ctx: &Context,
    chain_code: &str,
) -> Option<(String, Duration)> {
    breaker_open_detail_for_chain_code_with_ctx(ctx, chain_code)
        .await
        .map(|detail| (detail.host, detail.remaining))
}

pub struct ChainRpcBreakerOpenDetail {
    pub host: String,
    pub remaining: Duration,
    pub last_error_code: Option<u16>,
}

pub async fn breaker_open_detail_for_chain_code_with_ctx(
    ctx: &Context,
    chain_code: &str,
) -> Option<ChainRpcBreakerOpenDetail> {
    let host = guarded_host_for_chain_code(chain_code, ctx).await?;
    let breaker = breaker_for_host(&host);
    if breaker.is_open() {
        let remaining = breaker.remaining().unwrap_or(Duration::from_secs(0));
        Some(ChainRpcBreakerOpenDetail {
            host,
            remaining,
            last_error_code: breaker.last_error_code(),
        })
    } else {
        None
    }
}

pub async fn acquire_if_guarded_with_ctx(
    ctx: &Context,
    chain_code: &str,
) -> Option<OwnedSemaphorePermit> {
    let _host = guarded_host_for_chain_code(chain_code, ctx).await?;
    let sem = shared_chain_rpc_semaphore();
    match sem.acquire_owned().await {
        Ok(p) => Some(p),
        Err(_) => {
            tracing::error!("chain rpc semaphore closed; skip guard acquire");
            None
        }
    }
}

async fn acquire_guarded_host_permit(
    host: String,
    sem: Arc<Semaphore>,
    timeout: Duration,
) -> ChainRpcGuardAcquire {
    let started_at = std::time::Instant::now();
    tracing::debug!(host = %host, timeout = ?timeout, "waiting for chain rpc guard permit");
    match tokio::time::timeout(timeout, sem.acquire_owned()).await {
        Ok(Ok(permit)) => {
            let waited = started_at.elapsed();
            ChainRpcGuardAcquire::Acquired {
                host: host.clone(),
                permit: ChainRpcGuardPermit::new(host, permit),
                waited,
            }
        }
        Ok(Err(_)) => {
            tracing::error!(host = %host, "chain rpc semaphore closed; skip guard acquire");
            ChainRpcGuardAcquire::Closed { host }
        }
        Err(_) => {
            tracing::warn!(
                observability_event = "chain_rpc_guard_acquire_timeout",
                reason = "guard_wait_timeout",
                host = %host,
                timeout = ?timeout,
                waited = ?started_at.elapsed(),
                "chain rpc guard permit wait timed out"
            );
            ChainRpcGuardAcquire::TimedOut { host, timeout }
        }
    }
}

pub async fn acquire_if_guarded_with_ctx_timeout(
    ctx: &Context,
    chain_code: &str,
) -> ChainRpcGuardAcquire {
    let Some(host) = guarded_host_for_chain_code(chain_code, ctx).await else {
        return ChainRpcGuardAcquire::NotGuarded;
    };
    acquire_guarded_host_permit(host, shared_chain_rpc_semaphore(), acquire_timeout()).await
}

pub async fn acquire_for_attempt_with_ctx(
    ctx: &Context,
    chain_code: &str,
    trade_no: &str,
    action: &str,
    source: &str,
) -> ChainRpcGuardAttempt {
    match acquire_if_guarded_with_ctx_timeout(ctx, chain_code).await {
        ChainRpcGuardAcquire::Acquired { host, permit, waited } => {
            tracing::info!(
                trade_no = %trade_no,
                chain_code = %chain_code,
                action = %action,
                host = %host,
                waited = ?waited,
                source = %source,
                "Acquired chain rpc guard permit"
            );
            ChainRpcGuardAttempt::Proceed(Some(permit))
        }
        ChainRpcGuardAcquire::TimedOut { host, timeout } => {
            tracing::warn!(
                observability_event = "chain_rpc_guard_acquire_timeout",
                reason = "guard_wait_timeout",
                trade_no = %trade_no,
                chain_code = %chain_code,
                action = %action,
                host = %host,
                timeout = ?timeout,
                source = %source,
                "Skip chain action: chain rpc guard permit wait timed out; scanner will retry"
            );
            ChainRpcGuardAttempt::Skip
        }
        ChainRpcGuardAcquire::Closed { host } => {
            tracing::warn!(
                observability_event = "chain_rpc_call_timeout",
                reason = "rpc_execution_timeout",
                trade_no = %trade_no,
                chain_code = %chain_code,
                action = %action,
                host = %host,
                source = %source,
                "Chain rpc guard closed; continue without permit"
            );
            ChainRpcGuardAttempt::Proceed(None)
        }
        ChainRpcGuardAcquire::NotGuarded => ChainRpcGuardAttempt::Proceed(None),
    }
}

pub async fn run_with_rpc_timeout<F, T>(
    future: F,
    chain_code: &str,
    trade_no: &str,
    action: &str,
    source: &str,
) -> Result<T, tokio::time::error::Elapsed>
where
    F: Future<Output = T>,
{
    run_with_rpc_timeout_duration(future, rpc_timeout(), chain_code, trade_no, action, source).await
}

pub async fn run_guarded_with_ctx<T, Fut>(
    ctx: &Context,
    chain_code: &str,
    trade_no: &str,
    action: &str,
    source: &str,
    future: Fut,
) -> Result<Option<T>, ServiceError>
where
    Fut: Future<Output = Result<T, ServiceError>>,
{
    if let Some((host, remaining)) = breaker_open_for_chain_code_with_ctx(ctx, chain_code).await {
        tracing::warn!(
            trade_no = %trade_no,
            chain_code = %chain_code,
            action = %action,
            host = %host,
            remaining = ?remaining,
            source = %source,
            "Skip chain rpc: circuit breaker open"
        );
        return Ok(None);
    }

    let _guard = match acquire_for_attempt_with_ctx(ctx, chain_code, trade_no, action, source).await
    {
        ChainRpcGuardAttempt::Proceed(permit) => permit,
        ChainRpcGuardAttempt::Skip => return Ok(None),
    };

    match run_with_rpc_timeout(future, chain_code, trade_no, action, source).await {
        Ok(Ok(value)) => {
            record_success_for_chain_code_with_ctx(ctx, chain_code).await;
            Ok(Some(value))
        }
        Ok(Err(err)) => {
            record_transient_failure_from_error(&err);
            Err(err)
        }
        Err(_) => Ok(None),
    }
}

async fn run_with_rpc_timeout_duration<F, T>(
    future: F,
    timeout: Duration,
    chain_code: &str,
    trade_no: &str,
    action: &str,
    source: &str,
) -> Result<T, tokio::time::error::Elapsed>
where
    F: Future<Output = T>,
{
    match tokio::time::timeout(timeout, future).await {
        Ok(result) => Ok(result),
        Err(err) => {
            tracing::warn!(
                trade_no = %trade_no,
                chain_code = %chain_code,
                action = %action,
                timeout = ?timeout,
                source = %source,
                "chain rpc call timed out"
            );
            Err(err)
        }
    }
}

pub async fn record_success_for_chain_code_with_ctx(ctx: &Context, chain_code: &str) {
    let Some(host) = guarded_host_for_chain_code(chain_code, ctx).await else { return };
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
    record_transient_failure_from_message(&s);
}

pub fn record_transient_failure_from_message(msg: &str) {
    let classification = classify_chain_rpc_error_message(msg);
    record_transient_failure_from_classification(&classification);
}

pub fn record_transient_failure_from_classification(classification: &ChainRpcErrorClassification) {
    if classification.is_service_unavailable {
        TRANSIENT_503_COUNT.fetch_add(1, Ordering::Relaxed);
        maybe_report_transient_503_count(RpcCircuitBreaker::now_ms());
    }

    if !classification.is_transient {
        return;
    }
    let Some(host) = classification.guarded_host.as_deref() else { return };
    breaker_for_host(host).record_failure(classification.error_code);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainRpcErrorContext {
    message: String,
    classification: ChainRpcErrorClassification,
}

impl ChainRpcErrorContext {
    pub fn from_display<E: std::fmt::Display>(err: &E) -> Self {
        Self::from_message(err.to_string())
    }

    pub fn from_message(message: impl Into<String>) -> Self {
        let message = message.into();
        let classification = classify_chain_rpc_error_message(&message);
        Self { message, classification }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn classification(&self) -> &ChainRpcErrorClassification {
        &self.classification
    }

    pub fn error_code(&self) -> Option<u16> {
        self.classification.error_code
    }

    pub fn guarded_host(&self) -> Option<&str> {
        self.classification.guarded_host.as_deref()
    }

    pub fn transient_reason(&self) -> Option<&'static str> {
        self.classification.transient_reason
    }

    pub fn is_service_unavailable(&self) -> bool {
        self.classification.is_service_unavailable
    }

    pub fn is_transient(&self) -> bool {
        self.classification.is_transient
    }

    pub fn record_transient_failure(&self) {
        record_transient_failure_from_classification(&self.classification);
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChainRpcFailureLog<'a> {
    chain_code: Option<&'a str>,
    rpc: Option<&'a str>,
    trade_no: Option<&'a str>,
    tx_hash: Option<&'a str>,
    source: Option<&'a str>,
    op: Option<&'a str>,
    visibility_kind: Option<&'a str>,
    attempt: Option<u32>,
    delay_ms: Option<u64>,
}

impl<'a> ChainRpcFailureLog<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn chain_code(mut self, chain_code: &'a str) -> Self {
        self.chain_code = Some(chain_code);
        self
    }

    pub fn rpc(mut self, rpc: &'a str) -> Self {
        self.rpc = Some(rpc);
        self
    }

    pub fn trade_no(mut self, trade_no: &'a str) -> Self {
        self.trade_no = Some(trade_no);
        self
    }

    pub fn tx_hash(mut self, tx_hash: &'a str) -> Self {
        self.tx_hash = Some(tx_hash);
        self
    }

    pub fn source(mut self, source: &'a str) -> Self {
        self.source = Some(source);
        self
    }

    pub fn op(mut self, op: &'a str) -> Self {
        self.op = Some(op);
        self
    }

    pub fn visibility_kind(mut self, visibility_kind: &'a str) -> Self {
        self.visibility_kind = Some(visibility_kind);
        self
    }

    pub fn attempt(mut self, attempt: u32) -> Self {
        self.attempt = Some(attempt);
        self
    }

    pub fn delay_ms(mut self, delay_ms: u64) -> Self {
        self.delay_ms = Some(delay_ms);
        self
    }

    pub fn warn(self, error: &ChainRpcErrorContext, reason: &str) {
        tracing::warn!(
            chain_code = self.chain_code.unwrap_or(""),
            rpc = self.rpc.unwrap_or(""),
            trade_no = self.trade_no.unwrap_or(""),
            tx_hash = self.tx_hash.unwrap_or(""),
            source = self.source.unwrap_or(""),
            op = self.op.unwrap_or(""),
            visibility_kind = self.visibility_kind.unwrap_or(""),
            attempt = ?self.attempt,
            delay_ms = ?self.delay_ms,
            error_code = ?error.error_code(),
            guarded_host = error.guarded_host().unwrap_or(""),
            transient_reason = error.transient_reason().unwrap_or(""),
            error = %error.message(),
            reason = %reason,
            "chain rpc failure"
        );
    }

    pub fn error(self, error: &ChainRpcErrorContext, reason: &str) {
        tracing::error!(
            chain_code = self.chain_code.unwrap_or(""),
            rpc = self.rpc.unwrap_or(""),
            trade_no = self.trade_no.unwrap_or(""),
            tx_hash = self.tx_hash.unwrap_or(""),
            source = self.source.unwrap_or(""),
            op = self.op.unwrap_or(""),
            visibility_kind = self.visibility_kind.unwrap_or(""),
            attempt = ?self.attempt,
            delay_ms = ?self.delay_ms,
            error_code = ?error.error_code(),
            guarded_host = error.guarded_host().unwrap_or(""),
            transient_reason = error.transient_reason().unwrap_or(""),
            error = %error.message(),
            reason = %reason,
            "chain rpc failure"
        );
    }
}

pub fn guarded_chain_rpc_host_from_message(msg: &str) -> Option<String> {
    GUARDED_HOSTS.iter().find(|h| msg.contains(h.as_str())).cloned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainRpcErrorClassification {
    pub error_code: Option<u16>,
    pub guarded_host: Option<String>,
    pub transient_reason: Option<&'static str>,
    pub is_service_unavailable: bool,
    pub is_html_error_page: bool,
    pub is_empty_response: bool,
    pub is_transient: bool,
}

pub fn classify_chain_rpc_error_message(msg: &str) -> ChainRpcErrorClassification {
    let error_code = chain_rpc_error_code(msg);
    let is_service_unavailable = error_code == Some(HTTP_STATUS_SERVICE_UNAVAILABLE);
    let is_html_error_page = is_chain_rpc_html_error_page(msg);
    let is_empty_response = is_empty_chain_rpc_response_message(msg);
    let transient_reason = if is_service_unavailable {
        Some("http_503")
    } else if is_html_error_page {
        Some("html_error_page")
    } else if is_empty_response {
        Some("empty_response")
    } else {
        None
    };
    ChainRpcErrorClassification {
        error_code,
        guarded_host: guarded_chain_rpc_host_from_message(msg),
        transient_reason,
        is_service_unavailable,
        is_html_error_page,
        is_empty_response,
        is_transient: is_service_unavailable || is_html_error_page || is_empty_response,
    }
}

pub fn chain_rpc_error_code(msg: &str) -> Option<u16> {
    for marker in ["code=", "code:"] {
        let Some((_, tail)) = msg.split_once(marker) else {
            continue;
        };
        let digits: String =
            tail.trim_start().chars().take_while(|ch| ch.is_ascii_digit()).collect();
        if let Ok(code) = digits.parse::<u16>() {
            return Some(code);
        }
    }
    None
}

pub fn is_empty_chain_rpc_response_message(msg: &str) -> bool {
    msg.contains("missing field `Error`")
        || msg.contains("value = {}")
        || msg.contains("all response:{}")
}

pub fn is_service_unavailable_error(msg: &str) -> bool {
    classify_chain_rpc_error_message(msg).is_service_unavailable
}

pub fn is_transient_chain_rpc_error_message(msg: &str) -> bool {
    // Transient patterns seen under pressure:
    // - HTTP 503 + HTML error pages
    // - Empty JSON / partial bodies causing deserialization errors
    classify_chain_rpc_error_message(msg).is_transient
}

pub fn is_chain_rpc_html_error_page(msg: &str) -> bool {
    msg.contains("<!DOCTYPE html>")
}

#[cfg(test)]
mod tests {
    use super::{
        ChainRpcGuardAcquire, RpcCircuitBreaker, acquire_guarded_host_permit, chain_rpc_error_code,
        classify_chain_rpc_error_message, guarded_chain_rpc_host_from_message,
        is_chain_rpc_html_error_page, is_empty_chain_rpc_response_message,
        is_service_unavailable_error, is_transient_chain_rpc_error_message,
        run_with_rpc_timeout_duration,
    };
    use std::{sync::Arc, thread, time::Duration};
    use tokio::sync::Semaphore;

    #[test]
    fn transient_chain_rpc_error_message_detects_503_and_html_payloads() {
        let nile_503 = "Node response error: code=503, rpc=https://api.nileex.io/wallet/triggerconstantcontract";
        assert!(is_transient_chain_rpc_error_message(nile_503));
        assert!(is_service_unavailable_error(nile_503));
        assert_eq!(
            chain_rpc_error_code(
                "Node response error: code=503, rpc=https://api.nileex.io/wallet/triggerconstantcontract"
            ),
            Some(503)
        );
        assert_eq!(
            chain_rpc_error_code(
                "NodeResponseError { code: 503, rpc: \"https://api.nileex.io/wallet/triggerconstantcontract\" }"
            ),
            Some(503)
        );
        assert_eq!(
            chain_rpc_error_code("Node response error: code=-32000, rpc=https://example"),
            None
        );
        assert_eq!(guarded_chain_rpc_host_from_message(nile_503).as_deref(), Some("api.nileex.io"));
        let classification = classify_chain_rpc_error_message(nile_503);
        assert_eq!(classification.error_code, Some(503));
        assert_eq!(classification.guarded_host.as_deref(), Some("api.nileex.io"));
        assert_eq!(classification.transient_reason, Some("http_503"));
        assert!(classification.is_service_unavailable);
        assert!(classification.is_transient);
        assert!(is_transient_chain_rpc_error_message(
            "all response:<!DOCTYPE html><html><body>nginx</body></html>"
        ));
        assert!(is_chain_rpc_html_error_page(
            "all response:<!DOCTYPE html><html><body>nginx</body></html>"
        ));
        assert!(is_empty_chain_rpc_response_message("all response:{}"));
        assert!(!is_transient_chain_rpc_error_message("rpc error: invalid address"));
    }

    #[test]
    fn rpc_circuit_breaker_opens_after_consecutive_failures_and_closes_on_success() {
        let breaker = RpcCircuitBreaker::default();
        for _ in 0..10 {
            breaker.record_failure(Some(503));
            thread::sleep(Duration::from_millis(1));
        }
        assert!(breaker.is_open());
        assert!(breaker.remaining().is_some());
        assert_eq!(breaker.last_error_code(), Some(503));

        breaker.record_success();
        assert!(!breaker.is_open());
        assert!(breaker.remaining().is_none());
        assert_eq!(breaker.last_error_code(), None);
    }

    #[tokio::test]
    async fn guarded_permit_acquire_times_out_without_permit() {
        let sem = Arc::new(Semaphore::new(0));
        let result =
            acquire_guarded_host_permit("api.nileex.io".to_string(), sem, Duration::from_millis(1))
                .await;

        assert!(matches!(
            result,
            ChainRpcGuardAcquire::TimedOut { host, timeout }
                if host == "api.nileex.io" && timeout == Duration::from_millis(1)
        ));
    }

    #[tokio::test]
    async fn guarded_permit_acquire_returns_permit_when_available() {
        let sem = Arc::new(Semaphore::new(1));
        let result =
            acquire_guarded_host_permit("api.nileex.io".to_string(), sem, Duration::from_secs(1))
                .await;

        assert!(matches!(
            result,
            ChainRpcGuardAcquire::Acquired { host, .. } if host == "api.nileex.io"
        ));
    }

    #[tokio::test]
    async fn rpc_timeout_returns_elapsed_for_slow_future() {
        let result: Result<u8, tokio::time::error::Elapsed> = run_with_rpc_timeout_duration(
            async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                1_u8
            },
            Duration::from_millis(1),
            "tron",
            "T_timeout",
            "broadcast",
            "test",
        )
        .await;

        assert!(result.is_err());
    }
}
