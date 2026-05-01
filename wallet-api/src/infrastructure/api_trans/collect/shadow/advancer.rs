use super::CollectIntent;
use crate::infrastructure::api_trans::{
    collect::{
        diagnose::{DiagnoseEvent, DiagnoseSource, DiagnoseStage, maybe_log_stuck},
        shadow::{ChainIntent, SideEffectIntent, stage::CollectStage},
    },
    shadow_rpc_policy,
};
use dashmap::DashMap;
use scopeguard::defer;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Semaphore, mpsc::Sender};
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;
use wallet_database::{ApiTransactionDbPool, repositories::api_wallet::collect::ApiCollectRepo};

/// 运行中记录的TTL（秒）
const RUNNING_TTL: Duration = Duration::from_secs(30);
/// Semaphore获取超时（秒）
const SEM_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(2);
/// DB连接获取超时（秒）
const DB_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(3);
/// DB查询超时（秒）
const DB_QUERY_TIMEOUT: Duration = Duration::from_secs(5);

/// 从CollectIntent中提取trade_no
fn intent_trade_no(intent: &CollectIntent) -> Option<String> {
    match intent {
        CollectIntent::Chain(chain_intent) => match chain_intent {
            ChainIntent::CheckResourceGate(trade_no) => Some(trade_no.clone()),
            ChainIntent::BuildTx(trade_no) => Some(trade_no.clone()),
            ChainIntent::BroadcastTx(trade_no) => Some(trade_no.clone()),
            ChainIntent::RecoverTx(trade_no) => Some(trade_no.clone()),
        },
        CollectIntent::SideEffect(side_effect_intent) => match side_effect_intent {
            SideEffectIntent::SendOrderAck(trade_no) => Some(trade_no.clone()),
            SideEffectIntent::SendResultAck(trade_no) => Some(trade_no.clone()),
            SideEffectIntent::UploadServiceFee(trade_no) => Some(trade_no.clone()),
            SideEffectIntent::UploadTxExecReceipt(trade_no) => Some(trade_no.clone()),
            SideEffectIntent::SendTxFeeResAck(trade_no) => Some(trade_no.clone()),
        },
    }
}

#[derive(Debug, Clone)]
pub struct ShadowAdvancer {
    pool: ApiTransactionDbPool,
    intent_tx: Sender<CollectIntent>,
    diagnose_tx: Option<Sender<DiagnoseEvent>>,
    running: Arc<DashMap<String, (Uuid, Instant)>>,
    last_gc: Arc<AtomicU64>,
    max_concurrency: Arc<AtomicUsize>,
    semaphore: Arc<Semaphore>,
}

impl ShadowAdvancer {
    pub fn new(
        pool: ApiTransactionDbPool,
        intent_tx: Sender<CollectIntent>,
        diagnose_tx: Option<Sender<DiagnoseEvent>>,
    ) -> Self {
        // 从环境变量读取最大并发数（SQLite 场景下默认保守）
        let max_concurrency = shadow_rpc_policy::read_usize_env("SHADOW_MAX_CONCURRENCY", 8, 4, 64);

        Self {
            pool,
            intent_tx,
            diagnose_tx,
            running: Arc::new(DashMap::new()),
            last_gc: Arc::new(AtomicU64::new(0)),
            max_concurrency: Arc::new(AtomicUsize::new(max_concurrency)),
            semaphore: Arc::new(Semaphore::new(max_concurrency)),
        }
    }

    pub fn configured_max_concurrency(&self) -> usize {
        self.max_concurrency.load(Ordering::Relaxed)
    }

    fn runtime_capacity_snapshot(&self) -> (u32, usize, usize, usize) {
        let pool = self.pool.as_ref();
        (
            pool.size(),
            pool.num_idle(),
            self.semaphore.available_permits(),
            self.max_concurrency.load(Ordering::Relaxed),
        )
    }

    /// 获取当前时间戳（毫秒）
    fn now_ms() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
    }

    /// 执行 running map 的 GC，清理过期记录
    fn gc_running(running: &Arc<DashMap<String, (Uuid, Instant)>>, last_gc: &Arc<AtomicU64>) {
        let now = Self::now_ms();
        let last_gc_time = last_gc.load(Ordering::Relaxed);

        // 每10秒执行一次GC
        if now - last_gc_time > 10000 {
            let before = running.len();
            running.retain(|_, (_, t)| t.elapsed() < RUNNING_TTL);
            let after = running.len();

            if before > after {
                trace!("Running map GC: removed {} expired entries", before - after);
            }

            last_gc.store(now, Ordering::Relaxed);
        }
    }

    /// 尝试基于当前事实推进一个阶段
    ///
    /// 注意：try_advance 每次最多推进一个阶段
    /// 多阶段推进依赖后续 Tick 或定时扫描
    ///
    /// 参数：
    /// - trade_no: 归集交易编号
    ///
    /// 行为：
    /// 1. 检查是否正在运行（带TTL）
    /// 2. 获取semaphore许可（带timeout）
    /// 3. 查询最新的DB状态（带timeout）
    /// 4. 基于事实状态，按照 ADVANCEMENT_ORDER 顺序检查可推进点
    /// 5. 找到第一个满足条件的推进点，生成对应意图
    /// 6. 发送意图并返回
    pub async fn try_advance(&self, trade_no: &str) {
        // 执行 running map GC
        let running = self.running.clone();
        let last_gc = self.last_gc.clone();
        Self::gc_running(&running, &last_gc);

        // 生成唯一guard id
        let guard_id = Uuid::new_v4();
        let trade_no_str = trade_no.to_string();

        // 检查是否正在运行（带TTL）
        let mut is_running = false;

        match running.entry(trade_no_str.clone()) {
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                // 不存在，插入
                entry.insert((guard_id, Instant::now()));
            }
            dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                // 存在，检查TTL
                let (_, ts) = entry.get();
                if ts.elapsed() < RUNNING_TTL {
                    // 未过期，跳过
                    debug!(trade_no = %trade_no, "Advance skipped: already running");
                    is_running = true;
                } else {
                    // 已过期，替换
                    entry.insert((guard_id, Instant::now()));
                }
            }
        }

        if is_running {
            return;
        }

        // 确保清理running记录
        defer! {
            // 只删除自己的guard，使用一次remove操作完成
            if let Some((_, (id, _))) = running.remove(&trade_no_str) {
                if id != guard_id {
                    // 如果不是自己的guard，重新插入
                    running.insert(trade_no_str.clone(), (id, Instant::now()));
                }
            }
        };

        // 获取semaphore许可（带timeout）
        let semaphore = self.semaphore.clone();
        let permit =
            match tokio::time::timeout(SEM_ACQUIRE_TIMEOUT, semaphore.acquire_owned()).await {
                Ok(Ok(permit)) => permit,
                Ok(Err(e)) => {
                    let (db_pool_size, db_pool_idle, sem_available, advancer_max_concurrency) =
                        self.runtime_capacity_snapshot();
                    error!(
                        trade_no = %trade_no,
                        error = %e,
                        db_pool_size,
                        db_pool_idle,
                        sem_available,
                        advancer_max_concurrency,
                        "Failed to acquire semaphore"
                    );
                    return;
                }
                Err(_) => {
                    let (db_pool_size, db_pool_idle, sem_available, advancer_max_concurrency) =
                        self.runtime_capacity_snapshot();
                    error!(
                        trade_no = %trade_no,
                        timeout = ?SEM_ACQUIRE_TIMEOUT,
                        db_pool_size,
                        db_pool_idle,
                        sem_available,
                        advancer_max_concurrency,
                        "Semaphore acquire timeout"
                    );
                    return;
                }
            };
        // 确保permit在作用域结束前不会被释放
        let _permit = permit;

        trace!(trade_no = %trade_no, "Try advancing collect transaction");

        // 查询最新的DB状态（带timeout）
        let pool = self.pool.clone();
        let collect = match tokio::time::timeout(
            DB_QUERY_TIMEOUT,
            ApiCollectRepo::get_api_collect_by_trade_no(&pool, trade_no),
        )
        .await
        {
            Ok(Ok(collect)) => collect,
            Ok(Err(e)) => {
                let (db_pool_size, db_pool_idle, sem_available, advancer_max_concurrency) =
                    self.runtime_capacity_snapshot();
                error!(
                    trade_no = %trade_no,
                    error = %e,
                    db_pool_size,
                    db_pool_idle,
                    sem_available,
                    advancer_max_concurrency,
                    "Failed to get api collect by trade_no"
                );
                return;
            }
            Err(_) => {
                let (db_pool_size, db_pool_idle, sem_available, advancer_max_concurrency) =
                    self.runtime_capacity_snapshot();
                error!(
                    trade_no = %trade_no,
                    timeout = ?DB_QUERY_TIMEOUT,
                    db_pool_size,
                    db_pool_idle,
                    sem_available,
                    advancer_max_concurrency,
                    "DB query timeout"
                );
                return;
            }
        };

        // 架构级保险丝：冻结或已终止的记录不允许推进
        if collect.finished_at.is_some() {
            debug!(trade_no = %trade_no, "Advance skipped: frozen or finished");
            return;
        }

        // err_code 冻结：只允许 UploadTxExecReceipt
        if collect.err_code.is_some() {
            let eval = crate::infrastructure::api_trans::collect::shadow::predicate::evaluate_stage(
                CollectStage::NeedTxExecReceiptUpload,
                &collect,
            );

            if eval.can_advance {
                info!(trade_no = %trade_no, "Need to upload tx exec receipt (err_code frozen state)");
                let intent = CollectIntent::SideEffect(SideEffectIntent::UploadTxExecReceipt(
                    trade_no.to_string(),
                ));
                self.dispatch_intent(intent);
            }
            return;
        }

        // 按照 COLLECT_ADVANCEMENT_ORDER 顺序检查可推进点
        // 顺序与 scan_round 完全一致，确保行为一致性
        for stage in
            crate::infrastructure::api_trans::collect::shadow::stage::COLLECT_ADVANCEMENT_ORDER
                .iter()
        {
            let eval = crate::infrastructure::api_trans::collect::shadow::predicate::evaluate_stage(
                *stage, &collect,
            );

            if eval.can_advance {
                match stage {
                    CollectStage::NeedOrderAck => {
                        info!(trade_no = %trade_no, "Need to send order ACK");
                        let intent = CollectIntent::SideEffect(SideEffectIntent::SendOrderAck(
                            trade_no.to_string(),
                        ));
                        self.dispatch_intent(intent);
                        return;
                    }
                    CollectStage::NeedResourceGate => {
                        info!(trade_no = %trade_no, "Need to check resource gate");
                        let intent = CollectIntent::Chain(ChainIntent::CheckResourceGate(
                            trade_no.to_string(),
                        ));
                        self.dispatch_intent(intent);
                        return;
                    }
                    CollectStage::CanBuild => {
                        info!(trade_no = %trade_no, "Can build transaction");
                        let intent =
                            CollectIntent::Chain(ChainIntent::BuildTx(trade_no.to_string()));
                        self.dispatch_intent(intent);
                        return;
                    }
                    CollectStage::NeedTxFeeResAck => {
                        info!(trade_no = %trade_no, "Need to send tx fee res ACK");
                        let intent = CollectIntent::SideEffect(SideEffectIntent::SendTxFeeResAck(
                            trade_no.to_string(),
                        ));
                        self.dispatch_intent(intent);
                        return;
                    }
                    CollectStage::CanBroadcast => {
                        if let Some((host, remaining)) =
                            shadow_rpc_policy::breaker_open_for_chain_code(&collect.chain_code)
                                .await
                        {
                            debug!(
                                trade_no = %trade_no,
                                chain_code = %collect.chain_code,
                                host = %host,
                                remaining = ?remaining,
                                "try_advance_skip_because_breaker_open: collect advancer broadcast skipped"
                            );
                            if shadow_rpc_policy::should_emit_breaker_warn(&format!(
                                "collect.advancer.try_advance.breaker:{}:{}",
                                collect.chain_code, host
                            )) {
                                warn!(
                                    trade_no = %trade_no,
                                    chain_code = %collect.chain_code,
                                    host = %host,
                                    remaining = ?remaining,
                                    "try_advance_skip_because_breaker_open: collect advancer broadcast skipped"
                                );
                            }
                            return;
                        }
                        info!(trade_no = %trade_no, "Can broadcast transaction");
                        let intent =
                            CollectIntent::Chain(ChainIntent::BroadcastTx(trade_no.to_string()));
                        self.dispatch_intent(intent);
                        return;
                    }
                    CollectStage::NeedRecover => {
                        if let Some((host, remaining)) =
                            shadow_rpc_policy::breaker_open_for_chain_code(&collect.chain_code)
                                .await
                        {
                            debug!(
                                trade_no = %trade_no,
                                chain_code = %collect.chain_code,
                                host = %host,
                                remaining = ?remaining,
                                "try_advance_skip_because_breaker_open: collect advancer recover skipped"
                            );
                            if shadow_rpc_policy::should_emit_breaker_warn(&format!(
                                "collect.advancer.try_advance.breaker:{}:{}",
                                collect.chain_code, host
                            )) {
                                warn!(
                                    trade_no = %trade_no,
                                    chain_code = %collect.chain_code,
                                    host = %host,
                                    remaining = ?remaining,
                                    "try_advance_skip_because_breaker_open: collect advancer recover skipped"
                                );
                            }
                            return;
                        }
                        if !shadow_rpc_policy::allow_recover_dispatch(&format!(
                            "collect_advancer:{trade_no}"
                        )) {
                            debug!(
                                trade_no = %trade_no,
                                cooldown = ?shadow_rpc_policy::recover_cooldown(),
                                "recover_skip_because_cooldown: collect advancer recover skipped"
                            );
                            return;
                        }
                        info!(trade_no = %trade_no, "Need to recover transaction");
                        let intent =
                            CollectIntent::Chain(ChainIntent::RecoverTx(trade_no.to_string()));
                        self.dispatch_intent(intent);
                        return;
                    }
                    CollectStage::NeedTxExecReceiptUpload => {
                        info!(trade_no = %trade_no, "Need to upload tx exec receipt");
                        let intent = CollectIntent::SideEffect(
                            SideEffectIntent::UploadTxExecReceipt(trade_no.to_string()),
                        );
                        self.dispatch_intent(intent);
                        return;
                    }
                    CollectStage::NeedResultAck => {
                        info!(trade_no = %trade_no, "Need to send result ACK");
                        let intent = CollectIntent::SideEffect(SideEffectIntent::SendResultAck(
                            trade_no.to_string(),
                        ));
                        self.dispatch_intent(intent);
                        return;
                    }
                    CollectStage::NeedServiceFeeUpload => {
                        info!(trade_no = %trade_no, "Need to upload service fee");
                        let intent = CollectIntent::SideEffect(SideEffectIntent::UploadServiceFee(
                            trade_no.to_string(),
                        ));
                        self.dispatch_intent(intent);
                        return;
                    }
                    CollectStage::FullyBlocked => {
                        // 不应该出现在推进顺序中
                        debug_assert!(false, "FullyBlocked must not appear in ADVANCEMENT_ORDER");
                        continue;
                    }
                }
            }
        }

        // 无可用推进点
        trace!(trade_no = %trade_no, "No advancement possible based on current facts");

        // 检查是否可能卡住
        let _ = maybe_log_stuck(
            &collect,
            &self.diagnose_tx,
            DiagnoseSource::Advancer,
            DiagnoseStage::Unknown,
        );
    }

    /// 分发推进意图
    fn dispatch_intent(&self, intent: CollectIntent) {
        debug!(?intent, "Generated collect intent");

        // 保存trade_no用于后续可能的诊断事件
        let trade_no = intent_trade_no(&intent);

        // 将意图发送给Dispatcher（非阻塞）
        if let Err(e) = self.intent_tx.try_send(intent) {
            error!("Failed to send collect intent: {}", e);

            // 触发scanner revisit
            if let Some(diagnose_tx) = &self.diagnose_tx {
                // 从保存的trade_no中提取
                if let Some(trade_no) = trade_no {
                    let meta = crate::infrastructure::api_trans::collect::diagnose::event::DiagnoseMeta::new(
                        trade_no,
                        crate::infrastructure::api_trans::collect::diagnose::event::DiagnoseSource::Advancer,
                        crate::infrastructure::api_trans::collect::diagnose::event::DiagnoseStage::Unknown,
                    );
                    if let Err(e) =
                        diagnose_tx.try_send(DiagnoseEvent::IntentDispatchFailed { meta })
                    {
                        error!("Failed to send IntentDispatchFailed event: {}", e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_try_advance() {
        // 创建测试用的通道
        let (intent_tx, _intent_rx) = mpsc::channel(100);
        let (diagnose_tx, _diagnose_rx) = mpsc::channel(100);

        // 创建测试用的内存数据库连接池
        // 注意：这里使用 sqlx 的内存数据库
        // 实际测试时，你可能需要使用真实的数据库连接
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let db_pool = std::sync::Arc::new(pool);
        let collect_pool = wallet_database::ApiTransactionDbPool::new(db_pool);

        // 创建 ShadowAdvancer 实例
        let advancer = ShadowAdvancer::new(collect_pool, intent_tx, Some(diagnose_tx));

        // 测试 try_advance 方法
        // 注意：由于我们使用的是内存数据库，这个测试可能会失败
        // 实际测试时需要提供一个真实的数据库连接并确保表结构存在
        let trade_no = "test_trade_no";
        advancer.try_advance(trade_no).await;
    }
}
