use crate::{error::service::ServiceError, domain::api_wallet::trans::ApiTransDomain};
use tracing::{info, error};
use dashmap::DashMap;
use once_cell::sync::OnceCell;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

pub enum BootstrapSource {
    ChainPending,
    Manual,
    Migration,
    Recovery,
}

impl BootstrapSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            BootstrapSource::ChainPending => "CHAIN_PENDING",
            BootstrapSource::Manual => "MANUAL",
            BootstrapSource::Migration => "MIGRATION",
            BootstrapSource::Recovery => "RECOVERY",
        }
    }
}

struct BootstrapState {
    in_progress: AtomicBool,
    completed: OnceCell<()>,
}

/// BootstrapGuard 确保在任何情况下都能自动解锁
struct BootstrapGuard {
    state: Arc<BootstrapState>,
}

impl BootstrapGuard {
    fn new(state: Arc<BootstrapState>) -> Option<Self> {
        // 尝试获取锁，只有当 in_progress 为 false 时才设置为 true
        if state.in_progress.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Some(Self { state })
        } else {
            None
        }
    }
}

impl Drop for BootstrapGuard {
    fn drop(&mut self) {
        // 释放锁，设置为 false
        self.state.in_progress.store(false, Ordering::Release);
    }
}

pub struct NonceBootstrapService {
    bootstrap_states: DashMap<(String, String), Arc<BootstrapState>>,
}

impl NonceBootstrapService {
    pub fn new() -> Self {
        Self {
            bootstrap_states: DashMap::new(),
        }
    }

    pub async fn ensure_nonce_initialized(
        &self,
        address: &str,
        chain: &str,
    ) -> Result<(), ServiceError> {
        // 检查地址锁状态（这里需要根据实际的锁管理实现）
        self.assert_address_locked(address, chain);

        let key = (address.to_string(), chain.to_string());
        let state = self.bootstrap_states
            .entry(key)
            .or_insert_with(|| Arc::new(BootstrapState {
                in_progress: AtomicBool::new(false),
                completed: OnceCell::new(),
            }))
            .clone();

        // 检查是否已经完成过 bootstrap
        if state.completed.get().is_some() {
            return Ok(());
        }

        // 等待并获取执行权
        let mut backoff = 1;
        let guard = loop {
            if let Some(guard) = BootstrapGuard::new(state.clone()) {
                break guard;
            }

            // 使用 yield_now + backoff 避免 CPU spin
            if backoff <= 4 {
                tokio::task::yield_now().await;
            } else {
                tokio::time::sleep(tokio::time::Duration::from_millis(backoff)).await;
            }

            backoff = (backoff * 2).min(32);
        };

        // double-check locking：拿到 guard 后再检查一次
        if state.completed.get().is_some() {
            return Ok(());
        }

        // 单出口结构，确保在任何情况下都能释放锁
        let result = async {
            // 实现具体的 bootstrap 逻辑
            let result = self.bootstrap_address(&address, &chain).await;

            // 标记完成
            if result.is_ok() {
                state.completed.set(()).unwrap_or(());
            }

            result
        }.await;

        result
    }

    async fn bootstrap_address(
        &self,
        address: &str,
        chain: &str,
    ) -> Result<(), ServiceError> {
        use wallet_database::repositories::api_wallet::nonce::ApiNonceRepo;
        
        // 从链上获取 pending nonce
        let chain_nonce = ApiTransDomain::nonce(address, chain).await?;
        info!(address = %address, chain = %chain, chain_nonce = %chain_nonce, source = "nonce_bootstrap", "Got chain nonce for bootstrap");

        // 获取数据库连接池
        let pool = crate::get_context()?.api_funds_pool()?;

        // 插入 bootstrap 记录
        let nonce = ApiNonceRepo::upsert_and_get_api_nonce(&pool, address, chain, chain_nonce as i32).await?;
        info!(address = %address, chain = %chain, nonce = %nonce, source = "nonce_bootstrap", "Bootstrap successful");
        Ok(())
    }

    fn assert_address_locked(&self, address: &str, chain: &str) {
        // TODO: 实现地址锁定检查
        // 注意：bootstrap 是 slow path，不要在这里 panic，否则会导致交易失败
        // 应该使用适当的错误处理机制
    }
}

// 全局服务实例
static NONCE_BOOTSTRAP_SERVICE: OnceCell<Arc<NonceBootstrapService>> = OnceCell::new();

pub fn get_nonce_bootstrap_service() -> Arc<NonceBootstrapService> {
    NONCE_BOOTSTRAP_SERVICE.get_or_init(|| Arc::new(NonceBootstrapService::new())).clone()
}

pub fn init_nonce_bootstrap_service() {
    NONCE_BOOTSTRAP_SERVICE.get_or_init(|| Arc::new(NonceBootstrapService::new()));
}
