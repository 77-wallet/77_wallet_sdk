use crate::{domain::api_wallet::trans::ApiTransDomain, error::service::ServiceError};
use tokio::time::{self, Duration};
use tracing::{error, info, warn};

pub enum AddressType {
    Active,
    Cold,
    Error,
}

pub struct PendingNonceReconciler {
    // 这里可以添加必要的依赖，如数据库连接池等
}

impl PendingNonceReconciler {
    pub fn new() -> Self {
        Self {
            // 初始化依赖
        }
    }

    pub async fn start(&self) {
        warn!(source = "pending_nonce_reconciler", "Pending nonce reconciler is now trigger-only, no background tasks");
        // 不启动任何后台任务，只响应触发
        return;
    }

    /// 触发 reconcile 单个地址
    pub async fn trigger_reconcile(&self, address: &str, chain: &str) {
        info!(address = %address, chain = %chain, source = "pending_nonce_reconciler", "Triggering reconcile for address");
        
        // 从多个 RPC 节点获取 nonce 并取最大值
        let chain_nonce = self.get_max_chain_nonce(address, chain).await;

        match chain_nonce {
            Ok(chain_nonce) => {
                info!(address = %address, chain = %chain, chain_nonce = %chain_nonce, source = "pending_nonce_reconciler", "Got max chain nonce");

                // 获取数据库中的 nonce
                let db_nonce = self.get_db_nonce(address, chain).await;

                match db_nonce {
                    Ok(db_nonce) => {
                        if chain_nonce > db_nonce {
                            info!(address = %address, chain = %chain, db_nonce = %db_nonce, chain_nonce = %chain_nonce, source = "pending_nonce_reconciler", "Nonce drift detected, updating");

                            // 更新数据库中的 nonce
                            if let Err(e) = self.update_db_nonce(address, chain, chain_nonce).await
                            {
                                error!(address = %address, chain = %chain, error = %e, source = "pending_nonce_reconciler", "Failed to update db nonce");
                            }
                        }
                    }
                    Err(e) => {
                        error!(address = %address, chain = %chain, error = %e, source = "pending_nonce_reconciler", "Failed to get db nonce");
                    }
                }
            }
            Err(e) => {
                error!(address = %address, chain = %chain, error = %e, source = "pending_nonce_reconciler", "Failed to get chain nonce");
            }
        }
    }

    /// 从多个 RPC 节点获取 nonce 并取最大值
    async fn get_max_chain_nonce(&self, address: &str, chain: &str) -> Result<u64, ServiceError> {
        use tokio::try_join;

        // 模拟从多个 RPC 节点获取 nonce
        // 实际实现时应该从配置的多个 RPC 节点获取
        let (nonce1, nonce2, nonce3) = try_join!(
            ApiTransDomain::nonce(address, chain),
            ApiTransDomain::nonce(address, chain),
            ApiTransDomain::nonce(address, chain)
        )?;

        // 取最大值
        let max_nonce = nonce1.max(nonce2).max(nonce3);
        info!(address = %address, chain = %chain, nonce1 = %nonce1, nonce2 = %nonce2, nonce3 = %nonce3, max_nonce = %max_nonce, source = "pending_nonce_reconciler", "Got nonces from multiple RPCs");

        Ok(max_nonce)
    }

    /// 获取数据库中的 nonce
    async fn get_db_nonce(&self, address: &str, chain: &str) -> Result<u64, ServiceError> {
        use wallet_database::repositories::api_wallet::nonce::ApiNonceRepo;

        // 获取数据库连接池
        let pool = crate::get_context()?.api_funds_pool()?;

        let nonce = ApiNonceRepo::get_api_nonce(&pool, address, chain).await? as u64;

        Ok(nonce)
    }

    /// 更新数据库中的 nonce
    async fn update_db_nonce(
        &self,
        address: &str,
        chain: &str,
        nonce: u64,
    ) -> Result<(), ServiceError> {
        use wallet_database::repositories::api_wallet::nonce::ApiNonceRepo;

        // 获取数据库连接池
        let pool = crate::get_context()?.api_funds_pool()?;

        let nonce =
            ApiNonceRepo::upsert_and_get_api_nonce(&pool, address, chain, nonce as i32).await?;
        info!(address = %address, chain = %chain, nonce = %nonce, source = "pending_nonce_reconciler", "Nonce updated successfully");
        Ok(())
    }
}

// 全局服务实例
use once_cell::sync::OnceCell;
use std::sync::Arc;

static PENDING_NONCE_RECONCILER: OnceCell<Arc<PendingNonceReconciler>> = OnceCell::new();

pub fn get_pending_nonce_reconciler() -> Arc<PendingNonceReconciler> {
    PENDING_NONCE_RECONCILER.get_or_init(|| Arc::new(PendingNonceReconciler::new())).clone()
}

pub fn init_pending_nonce_reconciler() {
    PENDING_NONCE_RECONCILER.get_or_init(|| Arc::new(PendingNonceReconciler::new()));
}
