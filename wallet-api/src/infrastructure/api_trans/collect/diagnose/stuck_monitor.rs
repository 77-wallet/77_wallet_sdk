use rand::Rng;
use std::time::{Duration, Instant};
use tracing::{error, warn};
use wallet_database::{CollectDbPool, entities::api_collect::ApiCollectEntity};

use super::{
    CachedDiagnoser,
    engine::diagnose_collect,
    event::{DiagnoseEvent, DiagnoseEventSender, DiagnoseSource, DiagnoseStage},
};

/// 默认冷却时间：用于 Advancer/Scanner 主路径的“卡住诊断”日志与事件发送
const DEFAULT_COOLDOWN_DURATION: Duration = Duration::from_secs(30);

/// 周期性扫描日志冷却时间：用于抑制同一笔交易在短时间内重复刷屏
const PERIODIC_LOG_COOLDOWN_DURATION: Duration = Duration::from_secs(10 * 60);

/// 最大冷却映射大小
const MAX_COOLDOWN_SIZE: usize = 20_000;

/// 硬上限冷却映射大小
const HARD_MAX_COOLDOWN_SIZE: usize = 50_000;

lazy_static::lazy_static! {
    /// 卡住诊断的冷却时间映射
    /// 用于防止短时间内对同一笔交易重复诊断
    pub static ref STUCK_COOLDOWN_MAP: dashmap::DashMap<std::sync::Arc<str>, std::time::Instant> = dashmap::DashMap::new();
    pub static ref COOLDOWN_KEY_QUEUE: std::sync::Mutex<std::collections::VecDeque<(std::sync::Arc<str>, std::time::Instant)>> = std::sync::Mutex::new(std::collections::VecDeque::new());
}

/// 全局诊断速率限制（每秒）
const GLOBAL_DIAGNOSE_QPS: u64 = 100;

/// 每阶段最小保障额度（每秒）
const MIN_STAGE_QPS: u64 = 5;

/// 诊断事件丢失监控
lazy_static::lazy_static! {
    pub static ref DROP_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    pub static ref LAST_DROP_AT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    pub static ref MAX_BURST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    pub static ref LAST_SUCCESS_SEND_AT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    pub static ref CURRENT_BURST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
}

/// 速率限制桶
pub struct RateLimitBucket {
    epoch: u64,
    count: u64,
}

/// 按阶段分桶的速率限制
lazy_static::lazy_static! {
    pub static ref STAGE_RATE_LIMITS: std::sync::Mutex<[RateLimitBucket; 8]> = std::sync::Mutex::new([
        RateLimitBucket { epoch: 0, count: 0 },
        RateLimitBucket { epoch: 0, count: 0 },
        RateLimitBucket { epoch: 0, count: 0 },
        RateLimitBucket { epoch: 0, count: 0 },
        RateLimitBucket { epoch: 0, count: 0 },
        RateLimitBucket { epoch: 0, count: 0 },
        RateLimitBucket { epoch: 0, count: 0 },
        RateLimitBucket { epoch: 0, count: 0 },
    ]);
    pub static ref GLOBAL_RATE_LIMIT: std::sync::Mutex<RateLimitBucket> = std::sync::Mutex::new(RateLimitBucket {
        epoch: 0,
        count: 0
    });
    pub static ref CURRENT_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
}

/// 获取当前时间戳（秒）
fn now_epoch() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

/// 检查全局速率限制
fn check_global_rate_limit() -> bool {
    let current_epoch = now_epoch();
    CURRENT_EPOCH.store(current_epoch, std::sync::atomic::Ordering::Relaxed);

    let mut bucket = GLOBAL_RATE_LIMIT.lock().unwrap();

    // 重置桶
    if bucket.epoch != current_epoch {
        bucket.epoch = current_epoch;
        bucket.count = 0;
    }

    // 检查速率限制
    let count = bucket.count;
    bucket.count += 1;
    count < GLOBAL_DIAGNOSE_QPS
}

/// 检查阶段速率限制
fn check_stage_rate_limit(stage: DiagnoseStage) -> bool {
    let current_epoch = now_epoch();
    CURRENT_EPOCH.store(current_epoch, std::sync::atomic::Ordering::Relaxed);

    let stage_idx = match stage {
        DiagnoseStage::OrderAck => 0,
        DiagnoseStage::Build => 1,
        DiagnoseStage::Broadcast => 2,
        DiagnoseStage::Recover => 3,
        DiagnoseStage::ResultAck => 4,
        DiagnoseStage::ServiceFeeUpload => 5,
        DiagnoseStage::TxFeeResAck => 6,
        DiagnoseStage::TxExecReceipt => 7,
        DiagnoseStage::Unknown => 0,
    };

    let mut buckets = STAGE_RATE_LIMITS.lock().unwrap();
    let bucket = &mut buckets[stage_idx];

    // 重置桶
    if bucket.epoch != current_epoch {
        bucket.epoch = current_epoch;
        bucket.count = 0;
    }

    // 检查速率限制
    let count = bucket.count;
    bucket.count += 1;
    count < MIN_STAGE_QPS
}

/// 检查速率限制
fn check_rate_limit(stage: DiagnoseStage) -> bool {
    // 先检查全局速率限制
    if !check_global_rate_limit() {
        return false;
    }

    // 再检查阶段速率限制
    check_stage_rate_limit(stage)
}

/// 检查系统压力
fn check_system_overload() -> bool {
    // 这里使用简化的系统压力检查
    // 实际生产环境应该使用更复杂的指标
    false
}

/// 规范阶段（压缩枚举）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanonicalStage {
    OrderAck,
    Build,
    Broadcast,
    Recover,
    ResultAck,
    ServiceFeeUpload,
    TxFeeResAck,
    TxExecReceipt,
    Unknown,
}

/// 将 CollectStage 转换为 DiagnoseStage
pub fn collect_stage_to_diagnose_stage(
    stage: crate::infrastructure::api_trans::collect::shadow::stage::CollectStage,
) -> DiagnoseStage {
    match stage {
        crate::infrastructure::api_trans::collect::shadow::stage::CollectStage::NeedOrderAck => {
            DiagnoseStage::OrderAck
        }
        crate::infrastructure::api_trans::collect::shadow::stage::CollectStage::CanBuild => {
            DiagnoseStage::Build
        }
        crate::infrastructure::api_trans::collect::shadow::stage::CollectStage::NeedTxFeeResAck => {
            DiagnoseStage::TxFeeResAck
        }
        crate::infrastructure::api_trans::collect::shadow::stage::CollectStage::CanBroadcast => {
            DiagnoseStage::Broadcast
        }
        crate::infrastructure::api_trans::collect::shadow::stage::CollectStage::NeedRecover => {
            DiagnoseStage::Recover
        }
        crate::infrastructure::api_trans::collect::shadow::stage::CollectStage::NeedTxExecReceiptUpload => {
            DiagnoseStage::TxExecReceipt
        }
        crate::infrastructure::api_trans::collect::shadow::stage::CollectStage::NeedResultAck => {
            DiagnoseStage::ResultAck
        }
        crate::infrastructure::api_trans::collect::shadow::stage::CollectStage::NeedServiceFeeUpload => {
            DiagnoseStage::ServiceFeeUpload
        }
        crate::infrastructure::api_trans::collect::shadow::stage::CollectStage::FullyBlocked => {
            DiagnoseStage::Unknown
        }
    }
}

/// 将诊断阶段压缩为规范阶段
pub fn canonical_stage(stage: DiagnoseStage) -> CanonicalStage {
    match stage {
        DiagnoseStage::OrderAck => CanonicalStage::OrderAck,
        DiagnoseStage::Build => CanonicalStage::Build,
        DiagnoseStage::Broadcast => CanonicalStage::Broadcast,
        DiagnoseStage::Recover => CanonicalStage::Recover,
        DiagnoseStage::ResultAck => CanonicalStage::ResultAck,
        DiagnoseStage::ServiceFeeUpload => CanonicalStage::ServiceFeeUpload,
        DiagnoseStage::TxFeeResAck => CanonicalStage::TxFeeResAck,
        DiagnoseStage::TxExecReceipt => CanonicalStage::TxExecReceipt,
        DiagnoseStage::Unknown => CanonicalStage::Unknown,
    }
}

/// 生成冷却键
fn cooldown_key(trade_no: &str, stage: CanonicalStage) -> std::sync::Arc<str> {
    let mut key = String::with_capacity(64);
    key.push_str(trade_no);
    key.push_str("-");
    key.push_str(&format!("{:?}", stage));
    std::sync::Arc::<str>::from(key.into_boxed_str())
}

/// 检查是否应该诊断卡住
/// 包含冷却时间检查和自动清理
fn should_diagnose(trade_no: &str, stage: DiagnoseStage, cooldown: Duration) -> bool {
    let canonical = canonical_stage(stage);
    let key = cooldown_key(trade_no, canonical);
    let now = std::time::Instant::now();

    // 清理过期项和硬上限（只获取一次锁）
    let mut queue = COOLDOWN_KEY_QUEUE.lock().unwrap();

    // 清理过期项（O(1) 渐进式 GC）
    if STUCK_COOLDOWN_MAP.len() > MAX_COOLDOWN_SIZE {
        let mut removed = 0;
        while removed < 50 && !queue.is_empty() {
            if let Some((old_key, old_ts)) = queue.pop_front() {
                if let Some(entry) = STUCK_COOLDOWN_MAP.get(&old_key) {
                    if *entry == old_ts {
                        STUCK_COOLDOWN_MAP.remove(&old_key);
                        removed += 1;
                    }
                }
            }
        }
    }

    // 硬上限清理
    if queue.len() > HARD_MAX_COOLDOWN_SIZE {
        let remove_count = queue.len() / 10;
        for _ in 0..remove_count {
            if let Some((old_key, old_ts)) = queue.pop_front() {
                if let Some(entry) = STUCK_COOLDOWN_MAP.get(&old_key) {
                    if *entry == old_ts {
                        STUCK_COOLDOWN_MAP.remove(&old_key);
                    }
                }
            }
        }
    }

    // 检查冷却
    if let Some(entry) = STUCK_COOLDOWN_MAP.get(&key) {
        if now.duration_since(*entry) < cooldown {
            return false;
        }
    }

    // 更新冷却时间
    STUCK_COOLDOWN_MAP.insert(key.clone(), now);
    queue.push_back((key, now));
    true
}

pub struct CollectStuckMonitor {
    pool: CollectDbPool,
    min_interval: Duration,
    max_interval: Duration,
    current_interval: Duration,
    limit: usize,
    max_scan_duration: Duration,
    last_found_count: usize,
    diagnose_tx: Option<DiagnoseEventSender>,
    cached_diagnoser: CachedDiagnoser,
}

impl CollectStuckMonitor {
    pub fn new(
        pool: CollectDbPool,
        min_interval: Duration,
        max_interval: Duration,
        limit: usize,
        max_scan_duration: Duration,
    ) -> Self {
        Self {
            pool,
            min_interval,
            max_interval,
            current_interval: min_interval, // 初始使用最小间隔
            limit,
            max_scan_duration,
            last_found_count: 0,
            diagnose_tx: None,
            cached_diagnoser: CachedDiagnoser::default(),
        }
    }

    pub fn with_diagnose_tx(mut self, diagnose_tx: DiagnoseEventSender) -> Self {
        self.diagnose_tx = Some(diagnose_tx);
        self
    }

    pub async fn run_loop(&mut self, mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) {
        let mut next_deadline = Instant::now() + self.current_interval;

        loop {
            tokio::select! {
                // 接收关闭信号
                _ = shutdown_rx.recv() => {
                    return;
                },
                // 定时执行扫描
                _ = tokio::time::sleep_until(next_deadline.into()) => {
                    let now = Instant::now();

                    // 检查是否落后过多，需要重同步
                    if now > next_deadline + 3 * self.current_interval {
                        next_deadline = now + self.current_interval;
                    } else {
                        next_deadline += self.current_interval;
                    }

                    // 生成随机 jitter，避免多实例同时扫描
                    let jitter = {
                        let mut rng = rand::thread_rng();
                        Duration::from_millis(rng.gen_range(0..5000))
                    };
                    tokio::time::sleep(jitter).await;

                    if let Err(e) = self.scan_stuck_collects().await {
                        error!(error = %e, "Failed to scan stuck collects");
                    }

                    // 调整下一次扫描间隔
                    self.adjust_interval();
                },
            }
        }
    }

    async fn scan_stuck_collects(&mut self) -> anyhow::Result<()> {
        let start = Instant::now();

        // 使用 DB 预筛选获取可能卡住的交易
        let all_records = wallet_database::repositories::api_wallet::collect::ApiCollectRepo::scan_possible_stuck(
            &self.pool,
            self.limit,
        ).await?;

        // 过滤出确实卡住的交易
        let records: Vec<_> = all_records
            .into_iter()
            .filter(|r| {
                // 检查是否确实卡住
                let diag = self.cached_diagnoser.diagnose(r);
                diag.stuck_score >= 2
            })
            .collect();

        let mut found_count = 0;

        for r in records {
            // 检查扫描时间是否超过限制
            if start.elapsed() > self.max_scan_duration {
                warn!("Stuck scan exceeded max duration, stopping early");
                break;
            }

            let diag = self.cached_diagnoser.diagnose(&r);
            let real_stage = collect_stage_to_diagnose_stage(diag.stage);

            // 周期性扫描刷屏保护：同一 trade_no + stage 在冷却时间内只打一次
            if !should_diagnose(&r.trade_no, real_stage, PERIODIC_LOG_COOLDOWN_DURATION) {
                continue;
            }

            warn!(
                trade_no = %r.trade_no,
                stage = ?real_stage,
                reasons = ?diag.reasons,
                facts = %diag.facts_snapshot,
                score = diag.stuck_score,
                next_fact = ?diag.next_expected_fact,
                "🔥 Periodic stuck detection"
            );

            // 发送诊断事件
            if let Some(tx) = &self.diagnose_tx {
                let diag = self.cached_diagnoser.diagnose(&r);
                let meta = super::event::DiagnoseMeta::new(
                    r.trade_no.clone(),
                    super::event::DiagnoseSource::PeriodicScan,
                    real_stage,
                );
                if tx.try_send(DiagnoseEvent::PeriodicScan { meta, collect: r.clone() }).is_err() {
                    warn!(trade_no = %r.trade_no, "Failed to send diagnose event");
                }
            }

            found_count += 1;
        }

        // 更新最后发现的卡单数量
        self.last_found_count = found_count;

        Ok(())
    }

    /// 调整扫描间隔
    fn adjust_interval(&mut self) {
        if self.last_found_count > 20 {
            // 发现大量卡单，缩短间隔
            self.current_interval = self.min_interval;
        } else if self.last_found_count == 0 {
            // 没有发现卡单，延长间隔
            self.current_interval = std::cmp::min(self.current_interval * 2, self.max_interval);
        }
        // 否则保持当前间隔
    }
}

/// 诊断决策结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnoseDecision {
    /// 诊断事件已发送
    Sent,
    /// 因冷却时间跳过
    SkippedCooldown,
    /// 因速率限制跳过
    RateLimited,
    /// 通道已满
    ChannelFull,
    /// 因 backlog 过载跳过
    BacklogOverload,
    /// 因系统压力过载跳过
    SystemOverload,
    /// 因 actor 死亡跳过
    ActorDead,
    /// 终态记录，无需诊断
    Finished,
    /// 未达到卡住阈值
    NotStuck,
}

/// 快速诊断并可能记录日志（供 Advancer 和 Scanner 调用）
pub fn maybe_log_stuck(
    collect: &ApiCollectEntity,
    diagnose_tx: &Option<DiagnoseEventSender>,
    source: DiagnoseSource,
    stage: DiagnoseStage,
) -> DiagnoseDecision {
    // 终态二次检查
    if collect.finished_at.is_some() {
        return DiagnoseDecision::Finished;
    }

    // 获取真实诊断阶段
    let diag = diagnose_collect(collect);
    let real_stage = collect_stage_to_diagnose_stage(diag.stage);

    // 断言：如果传入的 stage 是 Unknown，那么真实阶段应该是一个有效的阶段
    debug_assert!(
        stage != DiagnoseStage::Unknown || real_stage != DiagnoseStage::Unknown,
        "Stage should not be Unknown when diagnose_collect is called"
    );

    // 检查系统压力
    if check_system_overload() {
        return DiagnoseDecision::SystemOverload;
    }

    // 检查速率限制
    if !check_rate_limit(real_stage) {
        return DiagnoseDecision::RateLimited;
    }

    // 冷却检查
    if !should_diagnose(&collect.trade_no, real_stage, DEFAULT_COOLDOWN_DURATION) {
        return DiagnoseDecision::SkippedCooldown;
    }

    // 检查是否达到卡住阈值
    if diag.stuck_score < 2 {
        return DiagnoseDecision::NotStuck;
    }

    // 记录警告日志
    warn!(
        stage = ?real_stage,
        source = ?source,
        reasons = ?diag.reasons,
        facts = %diag.facts_snapshot,
        score = diag.stuck_score,
        next_fact = ?diag.next_expected_fact,
        "⚠️ Collect order stuck diagnosis"
    );

    // 非阻塞发送诊断事件
    if let Some(tx) = diagnose_tx {
        let meta = super::event::DiagnoseMeta::new(collect.trade_no.clone(), source, real_stage);

        let event = DiagnoseEvent::NoAdvancement { meta, collect: collect.to_owned() };

        if tx.try_send(event).is_err() {
            // 更新丢失监控指标
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            DROP_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            LAST_DROP_AT.store(now, std::sync::atomic::Ordering::Relaxed);

            let current_burst =
                CURRENT_BURST.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let max_burst = MAX_BURST.load(std::sync::atomic::Ordering::Relaxed);
            if current_burst > max_burst {
                MAX_BURST.store(current_burst, std::sync::atomic::Ordering::Relaxed);
            }

            warn!(trade_no = %collect.trade_no, "Failed to send diagnose event");
            return DiagnoseDecision::ChannelFull;
        } else {
            // 更新成功发送时间
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            LAST_SUCCESS_SEND_AT.store(now, std::sync::atomic::Ordering::Relaxed);
            // 重置当前burst
            CURRENT_BURST.store(0, std::sync::atomic::Ordering::Relaxed);
        }
    }

    DiagnoseDecision::Sent
}

/// 快速诊断并可能记录日志（兼容旧接口）
pub fn maybe_log_stuck_compat(
    collect: &ApiCollectEntity,
    diagnose_tx: &Option<DiagnoseEventSender>,
) -> DiagnoseDecision {
    maybe_log_stuck(collect, diagnose_tx, DiagnoseSource::Advancer, DiagnoseStage::Unknown)
}
