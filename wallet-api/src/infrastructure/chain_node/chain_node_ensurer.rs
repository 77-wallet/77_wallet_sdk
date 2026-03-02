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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeCandidateSource {
    StrictFeature,
    FallbackMainnet,
    FallbackAny,
}

impl NodeCandidateSource {
    fn as_str(self) -> &'static str {
        match self {
            NodeCandidateSource::StrictFeature => "strict_feature",
            NodeCandidateSource::FallbackMainnet => "fallback_mainnet",
            NodeCandidateSource::FallbackAny => "fallback_any",
        }
    }
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
                    BusinessError::ChainNode(ChainNodeError::NoAvailableNode(
                        chain_code.to_string(),
                    ))
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
        let runtime_network = crate::context::get_context()?.chain_network();
        self.ensure_one_locked_inner_with_network(
            chain_code,
            status,
            node_id,
            is_core_chain,
            runtime_network.as_str(),
        )
        .await
    }

    async fn ensure_one_locked_inner_with_network(
        &self,
        chain_code: &str,
        status: u8,
        node_id: Option<&String>,
        is_core_chain: bool,
        runtime_network: &str,
    ) -> Result<(), ServiceError> {
        tracing::debug!(chain = %chain_code, node_id = ?node_id, "ensure_one_locked started");

        if status != 1 {
            tracing::debug!(chain = %chain_code, status = status, "chain not enabled, skip");
            return Ok(());
        }

        let (nodes, source) = self.load_nodes_for_chain(runtime_network, chain_code).await?;

        tracing::debug!(
            chain = %chain_code,
            available_nodes = nodes.len(),
            network = runtime_network,
            source = source.map(NodeCandidateSource::as_str).unwrap_or("none"),
            "nodes fetched"
        );

        if let Some(source) = source
            && source != NodeCandidateSource::StrictFeature
        {
            tracing::warn!(
                chain=%chain_code,
                runtime_network=runtime_network,
                fallback_source=source.as_str(),
                candidate_count=nodes.len(),
                "feature network nodes missing for chain; fallback node source is used"
            );
        }

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

    async fn load_nodes_for_chain(
        &self,
        runtime_network: &str,
        chain_code: &str,
    ) -> Result<
        (Vec<wallet_database::entities::node::NodeEntity>, Option<NodeCandidateSource>),
        ServiceError,
    > {
        let strict_nodes = NodeRepo::list_by_chain_with_network(
            &self.core_pool,
            chain_code,
            Some(runtime_network),
        )
        .await?
        .into_iter()
        .filter(|n| n.status == 1)
        .collect::<Vec<_>>();

        if !strict_nodes.is_empty() {
            return Ok((strict_nodes, Some(NodeCandidateSource::StrictFeature)));
        }

        let mainnet_nodes =
            NodeRepo::list_by_chain_with_network(&self.core_pool, chain_code, Some("mainnet"))
                .await?
                .into_iter()
                .filter(|n| n.status == 1)
                .collect::<Vec<_>>();

        if !mainnet_nodes.is_empty() {
            return Ok((mainnet_nodes, Some(NodeCandidateSource::FallbackMainnet)));
        }

        if Self::is_utxo_chain(chain_code) {
            return Ok((Vec::new(), None));
        }

        let any_nodes = NodeRepo::list_by_chain_with_network(&self.core_pool, chain_code, None)
            .await?
            .into_iter()
            .filter(|n| n.status == 1)
            .collect::<Vec<_>>();

        if !any_nodes.is_empty() {
            return Ok((any_nodes, Some(NodeCandidateSource::FallbackAny)));
        }

        Ok((Vec::new(), None))
    }

    fn is_utxo_chain(chain_code: &str) -> bool {
        matches!(chain_code.to_ascii_lowercase().as_str(), "btc" | "ltc" | "doge")
    }
}

#[cfg(test)]
mod tests {
    use super::{ChainNodeEnsurer, NodeCandidateSource};
    use wallet_database::entities::node::NodeEntity;

    fn node(node_id: &str) -> NodeEntity {
        NodeEntity {
            node_id: node_id.to_string(),
            name: "node".to_string(),
            chain_code: "tron".to_string(),
            rpc_url: "http://localhost".to_string(),
            ws_url: "".to_string(),
            http_url: "".to_string(),
            network: "testnet".to_string(),
            status: 1,
            is_local: 0,
            created_at: sqlx::types::chrono::Utc::now(),
            updated_at: None,
        }
    }

    fn pick_nodes_by_policy(
        chain_code: &str,
        strict_nodes: Vec<NodeEntity>,
        mainnet_nodes: Vec<NodeEntity>,
        any_nodes: Vec<NodeEntity>,
    ) -> (Vec<NodeEntity>, Option<NodeCandidateSource>) {
        if !strict_nodes.is_empty() {
            return (strict_nodes, Some(NodeCandidateSource::StrictFeature));
        }
        if !mainnet_nodes.is_empty() {
            return (mainnet_nodes, Some(NodeCandidateSource::FallbackMainnet));
        }
        if ChainNodeEnsurer::is_utxo_chain(chain_code) {
            return (Vec::new(), None);
        }
        if !any_nodes.is_empty() {
            return (any_nodes, Some(NodeCandidateSource::FallbackAny));
        }
        (Vec::new(), None)
    }

    #[test]
    fn testnet_strict_hit() {
        let (picked, source) = pick_nodes_by_policy(
            "tron",
            vec![node("testnet-1")],
            vec![node("mainnet-1")],
            vec![node("any-1")],
        );
        assert_eq!(source, Some(NodeCandidateSource::StrictFeature));
        assert_eq!(picked[0].node_id, "testnet-1");
    }

    #[test]
    fn testnet_fallback_mainnet_utxo() {
        let (picked, source) =
            pick_nodes_by_policy("btc", Vec::new(), vec![node("mainnet-btc")], vec![node("any")]);
        assert_eq!(source, Some(NodeCandidateSource::FallbackMainnet));
        assert_eq!(picked[0].node_id, "mainnet-btc");
    }

    #[test]
    fn testnet_fallback_any_non_utxo() {
        let (picked, source) = pick_nodes_by_policy(
            "eth",
            Vec::new(),
            Vec::new(),
            vec![node("non-standard-network-node")],
        );
        assert_eq!(source, Some(NodeCandidateSource::FallbackAny));
        assert_eq!(picked[0].node_id, "non-standard-network-node");
    }

    #[test]
    fn no_nodes_keep_empty() {
        let (picked, source) = pick_nodes_by_policy("tron", Vec::new(), Vec::new(), Vec::new());
        assert!(picked.is_empty());
        assert_eq!(source, None);
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod integration_tests {
    use super::ChainNodeEnsurer;
    use tempfile::TempDir;
    use wallet_database::{
        ApiWalletDbPool, CoreDbPool, SqliteContext,
        entities::{api_chain::NodeBindType, chain::ChainCreateVo, node::NodeCreateVo},
        repositories::{chain::ChainRepo, node::NodeRepo},
    };

    async fn setup_ensurer() -> (TempDir, CoreDbPool, ApiWalletDbPool, ChainNodeEnsurer) {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_root = temp_dir.path().to_string_lossy().to_string();
        let core_pool = SqliteContext::new(&db_root, Some("data.db"))
            .await
            .unwrap()
            .into_core_db_pool()
            .unwrap();
        let api_pool = SqliteContext::new(&db_root, Some("api_wallet.db"))
            .await
            .unwrap()
            .into_api_wallet_db_pool()
            .unwrap();
        let ensurer = ChainNodeEnsurer::new(core_pool.clone(), api_pool.clone());
        (temp_dir, core_pool, api_pool, ensurer)
    }

    async fn upsert_chain(core_pool: &CoreDbPool, chain_code: &str, main_symbol: &str) {
        let protocols = vec!["main".to_string()];
        let _ = ChainRepo::add(
            core_pool,
            ChainCreateVo::new(
                &format!("{chain_code}-name"),
                chain_code,
                &protocols,
                NodeBindType::AutoBackend,
                main_symbol,
            ),
        )
        .await
        .unwrap();
    }

    async fn upsert_node(core_pool: &CoreDbPool, node_id: &str, chain_code: &str, network: &str) {
        let _ = NodeRepo::upsert(
            core_pool,
            NodeCreateVo::new(node_id, node_id, chain_code, "http://127.0.0.1/rpc", None)
                .with_network(network)
                .with_is_local(0),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn sqlite_strict_hit_binds_feature_network_node() {
        let (_tmp, core_pool, _api_pool, ensurer) = setup_ensurer().await;
        upsert_chain(&core_pool, "tron", "TRX").await;
        upsert_node(&core_pool, "tron-testnet-node", "tron", "testnet").await;
        upsert_node(&core_pool, "tron-mainnet-node", "tron", "mainnet").await;

        let chain = ChainRepo::detail(&core_pool, "tron").await.unwrap().unwrap();
        assert!(chain.node_id.is_none());

        ensurer
            .ensure_one_locked_inner_with_network(
                &chain.chain_code,
                chain.status,
                chain.node_id.as_ref(),
                true,
                "testnet",
            )
            .await
            .unwrap();

        let chain_after = ChainRepo::detail(&core_pool, "tron").await.unwrap().unwrap();
        assert_eq!(chain_after.node_id.as_deref(), Some("tron-testnet-node"));
    }

    #[tokio::test]
    async fn sqlite_testnet_fallback_mainnet_for_utxo() {
        let (_tmp, core_pool, _api_pool, ensurer) = setup_ensurer().await;
        upsert_chain(&core_pool, "btc", "BTC").await;
        upsert_node(&core_pool, "btc-mainnet-node", "btc", "mainnet").await;

        let chain = ChainRepo::detail(&core_pool, "btc").await.unwrap().unwrap();
        assert!(chain.node_id.is_none());

        ensurer
            .ensure_one_locked_inner_with_network(
                &chain.chain_code,
                chain.status,
                chain.node_id.as_ref(),
                true,
                "testnet",
            )
            .await
            .unwrap();

        let chain_after = ChainRepo::detail(&core_pool, "btc").await.unwrap().unwrap();
        assert_eq!(chain_after.node_id.as_deref(), Some("btc-mainnet-node"));
    }

    #[tokio::test]
    async fn sqlite_testnet_fallback_any_for_non_utxo() {
        let (_tmp, core_pool, _api_pool, ensurer) = setup_ensurer().await;
        upsert_chain(&core_pool, "eth", "ETH").await;
        upsert_node(&core_pool, "eth-custom-node", "eth", "qa-net").await;

        let chain = ChainRepo::detail(&core_pool, "eth").await.unwrap().unwrap();
        assert!(chain.node_id.is_none());

        ensurer
            .ensure_one_locked_inner_with_network(
                &chain.chain_code,
                chain.status,
                chain.node_id.as_ref(),
                true,
                "testnet",
            )
            .await
            .unwrap();

        let chain_after = ChainRepo::detail(&core_pool, "eth").await.unwrap().unwrap();
        assert_eq!(chain_after.node_id.as_deref(), Some("eth-custom-node"));
    }

    #[tokio::test]
    async fn sqlite_no_nodes_keep_chain_node_id_empty() {
        let (_tmp, core_pool, _api_pool, ensurer) = setup_ensurer().await;
        upsert_chain(&core_pool, "sol", "SOL").await;

        let chain = ChainRepo::detail(&core_pool, "sol").await.unwrap().unwrap();
        assert!(chain.node_id.is_none());

        ensurer
            .ensure_one_locked_inner_with_network(
                &chain.chain_code,
                chain.status,
                chain.node_id.as_ref(),
                true,
                "testnet",
            )
            .await
            .unwrap();

        let chain_after = ChainRepo::detail(&core_pool, "sol").await.unwrap().unwrap();
        assert!(chain_after.node_id.is_none());
    }
}
