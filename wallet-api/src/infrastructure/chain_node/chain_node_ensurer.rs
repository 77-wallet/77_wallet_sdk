use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::Lazy;
/// 接管所有：
/// node_id + node_bind_type 的写入决策
use tokio::sync::Mutex;
use wallet_database::{
    DbPool,
    entities::{api_chain::NodeBindType, chain::ChainLike},
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
    pool: DbPool,
}

impl ChainNodeEnsurer {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// 启动 / 节点同步后调用
    pub async fn ensure_all(&self) -> Result<(), ServiceError> {
        let api_chains = ApiChainRepo::get_chain_list(&self.pool).await?;
        for c in api_chains {
            let code = c.chain_code().to_string();
            let _g = lock_for(&code);
            let _g = _g.lock().await;
            self.ensure_one_locked(&c).await?;
        }

        let chains = ChainRepo::get_chain_list(&self.pool).await?;
        for c in chains {
            let code = c.chain_code().to_string();
            let _g = lock_for(&code);
            let _g = _g.lock().await;
            self.ensure_one_locked(&c).await?;
        }

        Ok(())
    }

    /// 确保某条链绑定有效节点
    pub async fn ensure_chain(&self, chain_code: &str) -> Result<(), ServiceError> {
        let _g = lock_for(chain_code);
        let _g = _g.lock().await;
        if let Some(chain) = ApiChainRepo::detail(&self.pool, chain_code).await? {
            self.ensure_one_locked(&chain).await?;
        }

        if let Some(chain) = ChainRepo::detail(&self.pool, chain_code).await? {
            self.ensure_one_locked(&chain).await?;
        }
        Ok(())
    }

    /// 用户选择后，只需要调用 ensure 做一次校验
    pub async fn after_user_select(&self, chain_code: &str) -> Result<(), ServiceError> {
        self.ensure_chain(chain_code).await
    }

    /// 转账等强依赖节点场景：保证有 node 可用
    pub async fn ensure_and_get_node_chain(
        &self,
        chain_code: &str,
    ) -> Result<String, ServiceError> {
        let _g = lock_for(chain_code);
        let _g = _g.lock().await;

        let chain = ChainRepo::detail(&self.pool, chain_code)
            .await?
            .ok_or_else(|| BusinessError::Chain(ChainError::NotFound(chain_code.to_string())))?;

        self.ensure_one_locked(&chain).await?;

        let chain2 = ChainRepo::detail(&self.pool, chain_code).await?.unwrap();
        chain2.node_id.ok_or(
            BusinessError::ChainNode(ChainNodeError::NoAvailableNode(chain_code.to_string()))
                .into(),
        )
    }

    pub async fn ensure_and_get_node_api(&self, chain_code: &str) -> Result<String, ServiceError> {
        let _g = lock_for(chain_code);
        let _g = _g.lock().await;

        let chain = ApiChainRepo::detail(&self.pool, chain_code)
            .await?
            .ok_or_else(|| BusinessError::Chain(ChainError::NotFound(chain_code.to_string())))?;

        self.ensure_one_locked(&chain).await?;

        let chain2 = ApiChainRepo::detail(&self.pool, chain_code).await?.unwrap();
        chain2.node_id.ok_or(
            BusinessError::ChainNode(ChainNodeError::NoAvailableNode(chain_code.to_string()))
                .into(),
        )
    }

    /// 核心决策逻辑（锁内）
    async fn ensure_one_locked<C: ChainLike + Sync>(&self, chain: &C) -> Result<(), ServiceError> {
        let chain_code = chain.chain_code();

        // 只处理启用链
        if chain.status() != 1 {
            return Ok(());
        }

        // 拉取该链所有可用节点（含 backend + local）
        let nodes = NodeRepo::list(&self.pool, None)
            .await?
            .into_iter()
            .filter(
                |n| n.chain_code == *chain_code && n.status == 1, // TODO: 如果将来有 health/latency 等字段，这里一起判断
            )
            .collect::<Vec<_>>();

        if nodes.is_empty() {
            tracing::warn!(chain=%chain_code, "no available nodes for chain, keep node_id as is");
            return Ok(());
        }

        // 当前绑定是否仍然存在且可用
        let curr_valid = chain.node_id().and_then(|id| nodes.iter().find(|n| &n.node_id == id));

        if let Some(curr) = curr_valid {
            if curr.is_local == 0 {
                // 已经是 backend，直接保持
                return Ok(());
            }

            // 当前是 local，但如果现在有 backend，就升级
            let has_backend = nodes.iter().any(|n| n.is_local == 0);
            if !has_backend {
                return Ok(());
            }
        }

        // 👉 走到这里，说明：
        // - node_id 为 NULL
        // - 或者原节点已被删除/禁用
        // - 即使之前是 ManualUser，也允许被覆盖

        // 选一个：优先 backend(is_local=0)，否则 local
        let picked = nodes
            .iter()
            .find(|n| n.is_local == 0)
            .or_else(|| nodes.iter().find(|n| n.is_local == 1))
            .ok_or(BusinessError::ChainNode(ChainNodeError::NoAvailableNode(
                chain_code.to_string(),
            )))?;

        // 如果其实已经是这个 node，就不用再写库了
        if chain.node_id().as_deref() == Some(&picked.node_id) {
            return Ok(());
        }

        let bind_type =
            if picked.is_local == 0 { NodeBindType::AutoBackend } else { NodeBindType::AutoLocal };

        tracing::info!(
            chain=%chain_code,
            node=%picked.node_id,
            bind_type=?bind_type,
            "auto rebind chain to node"
        );

        C::set_node(&self.pool, chain_code, &picked.node_id, bind_type).await?;

        Ok(())
    }
}
