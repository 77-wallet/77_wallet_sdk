use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::Lazy;
/// 接管所有：
/// node_id + node_bind_type 的写入决策
use tokio::sync::Mutex;
use wallet_database::{
    ApiWalletDbPool, CoreDbPool,
    entities::{
        api_chain::{ApiChainEntity, NodeBindType},
        chain::ChainEntity,
    },
    repositories::{api_wallet::chain::ApiChainRepo, chain::ChainRepo, node::NodeRepo},
};

use crate::error::{
    business::{BusinessError, chain::ChainError, chain_node::ChainNodeError},
    service::ServiceError,
};

static ENSURE_LOCKS: Lazy<DashMap<String, Arc<Mutex<()>>>> = Lazy::new(DashMap::new);

fn lock_for(chain: &str) -> Arc<Mutex<()>> {
    ENSURE_LOCKS.entry(chain.to_string()).or_insert_with(|| Arc::new(Mutex::new(()))).clone()
}
pub struct ChainNodeEnsurer {
    core_pool: CoreDbPool,
    api_pool: ApiWalletDbPool,
}

impl ChainNodeEnsurer {
    pub fn new(core_pool: CoreDbPool, api_pool: ApiWalletDbPool) -> Self {
        Self { core_pool, api_pool }
    }

    /// 启动 / 节点同步后调用
    pub async fn ensure_all(&self) -> Result<(), ServiceError> {
        let api_chains = ApiChainRepo::get_chain_list(&self.api_pool).await?;
        tracing::debug!(count = api_chains.len(), "start ensure_all for api_chains");
        for c in api_chains {
            let code = c.chain_code.to_string();
            tracing::debug!(chain = %code, "ensure_all processing api_chain");
            let _g = lock_for(&code);
            let _g = _g.lock().await;
            self.ensure_one_locked_api(&c).await?;
        }

        let chains = ChainRepo::get_chain_list(&self.core_pool).await?;
        tracing::debug!(count = chains.len(), "start ensure_all for chains");
        for c in chains {
            let code = c.chain_code.to_string();
            tracing::debug!(chain = %code, "ensure_all processing chain");
            let _g = lock_for(&code);
            let _g = _g.lock().await;
            self.ensure_one_locked_core(&c).await?;
        }

        Ok(())
    }

    /// 确保某条链绑定有效节点
    pub async fn ensure_chain(&self, chain_code: &str) -> Result<(), ServiceError> {
        tracing::debug!(chain = %chain_code, "ensure_chain started");
        let _g = lock_for(chain_code);
        let _g = _g.lock().await;
        if let Some(chain) = ApiChainRepo::detail(&self.api_pool, chain_code).await? {
            tracing::debug!(chain = %chain_code, "ensure_chain processing api_chain");
            self.ensure_one_locked_api(&chain).await?;
        }

        if let Some(chain) = ChainRepo::detail(&self.core_pool, chain_code).await? {
            tracing::debug!(chain = %chain_code, "ensure_chain processing chain");
            self.ensure_one_locked_core(&chain).await?;
        }
        tracing::debug!(chain = %chain_code, "ensure_chain completed");
        Ok(())
    }

    /// 用户选择后，只需要调用 ensure 做一次校验
    pub async fn after_user_select(&self, chain_code: &str) -> Result<(), ServiceError> {
        self.ensure_chain(chain_code).await
    }

    /// 转账等强依赖节点场景：保证有 node 可用
    pub async fn ensure_and_get_standard_chain_node(
        &self,
        chain_code: &str,
    ) -> Result<String, ServiceError> {
        tracing::debug!(chain = %chain_code, "ensure_and_get_node_chain started");
        let _g = lock_for(chain_code);
        let _g = _g.lock().await;

        let chain = ChainRepo::detail(&self.core_pool, chain_code)
            .await?
            .ok_or_else(|| BusinessError::Chain(ChainError::NotFound(chain_code.to_string())))?;

        self.ensure_one_locked_core(&chain).await?;

        let chain2 = ChainRepo::detail(&self.core_pool, chain_code).await?.unwrap();
        let node_id = chain2.node_id.ok_or(BusinessError::ChainNode(
            ChainNodeError::NoAvailableNode(chain_code.to_string()),
        ))?;
        tracing::debug!(chain = %chain_code, node = %node_id, "ensure_and_get_node_chain completed");
        Ok(node_id)
    }

    pub async fn ensure_and_get_standard_chain_node_with_node(
        &self,
        chain_code: &str,
    ) -> Result<wallet_database::entities::chain::ChainWithNode, ServiceError> {
        tracing::debug!(chain = %chain_code, "ensure_and_get_chain_node_with_node started");
        let _g = lock_for(chain_code);
        let _g = _g.lock().await;

        let chain = ChainRepo::detail(&self.core_pool, chain_code)
            .await?
            .ok_or_else(|| BusinessError::Chain(ChainError::NotFound(chain_code.to_string())))?;

        self.ensure_one_locked_core(&chain).await?;

        let chain_with_node =
            ChainRepo::detail_with_node(&self.core_pool, chain_code).await?.ok_or_else(|| {
                BusinessError::ChainNode(ChainNodeError::NoAvailableNode(chain_code.to_string()))
            })?;

        tracing::debug!(chain = %chain_code, node = %chain_with_node.node_id, "ensure_and_get_chain_node_with_node completed");
        Ok(chain_with_node)
    }

    pub async fn ensure_and_get_api_chain_node(
        &self,
        chain_code: &str,
    ) -> Result<String, ServiceError> {
        tracing::debug!(chain = %chain_code, "ensure_and_get_node_api started");
        let _g = lock_for(chain_code);
        let _g = _g.lock().await;

        let chain = ApiChainRepo::detail(&self.api_pool, chain_code)
            .await?
            .ok_or_else(|| BusinessError::Chain(ChainError::NotFound(chain_code.to_string())))?;

        self.ensure_one_locked_api(&chain).await?;

        let chain2 = ApiChainRepo::detail(&self.api_pool, chain_code).await?.unwrap();
        let node_id = chain2.node_id.ok_or(BusinessError::ChainNode(
            ChainNodeError::NoAvailableNode(chain_code.to_string()),
        ))?;
        tracing::debug!(chain = %chain_code, node = %node_id, "ensure_and_get_node_api completed");
        Ok(node_id)
    }

    pub async fn ensure_and_get_api_chain_with_node(
        &self,
        chain_code: &str,
    ) -> Result<wallet_database::entities::chain::ChainWithNode, ServiceError> {
        tracing::debug!(chain = %chain_code, "ensure_and_get_chain_with_node started");
        let _g = lock_for(chain_code);
        let _g = _g.lock().await;

        let chain = ApiChainRepo::detail(&self.api_pool, chain_code)
            .await?
            .ok_or_else(|| BusinessError::Chain(ChainError::NotFound(chain_code.to_string())))?;

        self.ensure_one_locked_api(&chain).await?;

        let chain_with_node =
            ApiChainRepo::detail_with_node(&self.core_pool, &self.api_pool, chain_code)
                .await?
                .ok_or_else(|| {
                BusinessError::ChainNode(ChainNodeError::NoAvailableNode(chain_code.to_string()))
            })?;

        tracing::debug!(chain = %chain_code, node = %chain_with_node.node_id, "ensure_and_get_chain_with_node completed");
        Ok(chain_with_node)
    }

    /// 核心决策逻辑（锁内）
    async fn ensure_one_locked_core(&self, chain: &ChainEntity) -> Result<(), ServiceError> {
        self.ensure_one_locked_inner(&chain.chain_code, chain.status, chain.node_id.as_ref(), true)
            .await
    }

    async fn ensure_one_locked_api(&self, chain: &ApiChainEntity) -> Result<(), ServiceError> {
        self.ensure_one_locked_inner(&chain.chain_code, chain.status, chain.node_id.as_ref(), false)
            .await
    }

    async fn ensure_one_locked_inner(
        &self,
        chain_code: &str,
        status: u8,
        node_id: Option<&String>,
        is_core_chain: bool,
    ) -> Result<(), ServiceError> {
        tracing::debug!(chain = %chain_code, node_id = ?node_id, "ensure_one_locked started");

        if status != 1 {
            tracing::debug!(chain = %chain_code, status = status, "chain not enabled, skip");
            return Ok(());
        }

        let nodes = NodeRepo::list(&self.core_pool, None)
            .await?
            .into_iter()
            .filter(|n| n.chain_code == chain_code && n.status == 1)
            .collect::<Vec<_>>();

        tracing::debug!(chain = %chain_code, available_nodes = nodes.len(), "nodes fetched");

        if nodes.is_empty() {
            tracing::warn!(chain=%chain_code, "no available nodes for chain, keep node_id as is");
            return Ok(());
        }

        let curr_valid = node_id.and_then(|id| nodes.iter().find(|n| &n.node_id == id));

        if let Some(curr) = curr_valid {
            tracing::debug!(chain = %chain_code, current_node = %curr.node_id, is_local = curr.is_local, "current node is valid");
            if curr.is_local == 0 {
                tracing::debug!(chain = %chain_code, "current node is backend, keep as is");
                return Ok(());
            }

            let has_backend = nodes.iter().any(|n| n.is_local == 0);
            tracing::debug!(chain = %chain_code, has_backend = has_backend, "checking backend availability");
            if !has_backend {
                tracing::debug!(chain = %chain_code, "no backend available, keep local node");
                return Ok(());
            }
        } else {
            tracing::debug!(chain = %chain_code, "current node is null or invalid");
        }

        let picked = nodes
            .iter()
            .find(|n| n.is_local == 0)
            .or_else(|| nodes.iter().find(|n| n.is_local == 1))
            .ok_or(BusinessError::ChainNode(ChainNodeError::NoAvailableNode(
                chain_code.to_string(),
            )))?;

        tracing::info!(chain = %chain_code, picked_node = %picked.node_id, is_local = picked.is_local, "node selected for binding");

        if node_id.as_deref() == Some(&picked.node_id) {
            tracing::debug!(chain = %chain_code, "node already bound, no update needed");
            return Ok(());
        }

        let bind_type =
            if picked.is_local == 0 { NodeBindType::AutoBackend } else { NodeBindType::AutoLocal };

        tracing::debug!(
            chain=%chain_code,
            node=%picked.node_id,
            bind_type=?bind_type,
            "auto rebind chain to node"
        );

        if is_core_chain {
            ChainRepo::set_chain_node_with_type(
                &self.core_pool,
                chain_code,
                &picked.node_id,
                bind_type,
            )
            .await?;
        } else {
            ApiChainRepo::set_chain_node_with_type(
                &self.api_pool,
                chain_code,
                &picked.node_id,
                bind_type,
            )
            .await?;
        }

        Ok(())
    }
}
