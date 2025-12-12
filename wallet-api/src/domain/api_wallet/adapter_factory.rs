use crate::{
    domain::{
        api_wallet::adapter::{
            btc_tx::BtcTx, doge_tx::DogeTx, eth_tx::EthTx, ltx_tx::LtcTx, sol_tx::SolTx,
            sui_tx::SuiTx, ton_tx::TonTx, tron_tx::TronTx, tx::Tx,
        },
        chain::rpc_need_header,
    },
    error::{business::BusinessError, service::ServiceError},
};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::sync::Arc;
use wallet_database::{
    entities::chain::{ChainEntity, ChainWithNode},
    repositories::node::NodeRepo,
};
use wallet_types::chain::{chain::ChainCode, network::NetworkKind};

pub struct ApiChainAdapterFactory {
    transaction_adapter: DashMap<String, Arc<dyn Tx + Send + Sync>>,
}

// 创建全局单例
static API_CHAIN_ADAPTER_FACTORY: Lazy<Arc<ApiChainAdapterFactory>> =
    Lazy::new(|| Arc::new(ApiChainAdapterFactory { transaction_adapter: DashMap::new() }));

impl ApiChainAdapterFactory {
    /// 获取全局单例实例
    pub fn get_instance() -> Arc<ApiChainAdapterFactory> {
        API_CHAIN_ADAPTER_FACTORY.clone()
    }

    async fn get_chain_node(chain_code: ChainCode) -> Result<ChainWithNode, ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let node =
            ChainEntity::chain_node_info(pool.as_ref(), chain_code.to_string().as_str()).await?;
        if node.is_none() {
            tracing::error!("No node found in database: {}", chain_code);
            return Err(BusinessError::Chain(crate::error::business::chain::ChainError::NotFound(
                chain_code.to_string(),
            ))
            .into());
        }
        Ok(node.unwrap())
    }

    /// 预初始化所有链和节点的适配器
    pub async fn pre_init_all_adapters(&self) -> Result<(), ServiceError> {
        // 获取所有节点
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let all_nodes = NodeRepo::list(&pool, None).await?;

        tracing::info!(node_count = all_nodes.len(), "开始预初始化所有链和节点的适配器");

        // 为每个节点创建适配器
        for node in all_nodes {
            // 跳过状态为0的节点
            if node.status == 0 {
                continue;
            }

            let chain_code: ChainCode = match node.chain_code.as_str().try_into() {
                Ok(code) => code,
                Err(e) => {
                    tracing::warn!(chain_code=%node.chain_code, error=%e, "无法将链码转换为ChainCode枚举");
                    continue;
                }
            };

            // 使用chain_code和rpc_url组合作为缓存键
            let cache_key = format!("{}:{}", chain_code.to_string(), node.rpc_url);

            // 检查是否已经存在于缓存中
            if self.transaction_adapter.contains_key(&cache_key) {
                continue;
            }

            // 创建适配器
            tracing::info!(chain_code=%chain_code, rpc_url=%node.rpc_url, "预初始化transaction_adapter");

            let header_opt = if rpc_need_header(&node.rpc_url)? {
                Some(crate::context::CONTEXT.get().unwrap().get_rpc_header().await?)
            } else {
                None
            };

            let adapter: Arc<dyn Tx + Send + Sync> = match chain_code {
                ChainCode::Tron => Arc::new(TronTx::new(&node.rpc_url, header_opt)?),
                ChainCode::Bitcoin => Arc::new(BtcTx::new(&node.rpc_url, header_opt)?),
                ChainCode::Solana => Arc::new(SolTx::new(&node.rpc_url, header_opt)?),
                ChainCode::Ethereum => Arc::new(EthTx::new(
                    chain_code,
                    &node.rpc_url,
                    NetworkKind::from(node.network.as_str()),
                    header_opt,
                )?),
                ChainCode::BnbSmartChain => Arc::new(EthTx::new(
                    chain_code,
                    &node.rpc_url,
                    NetworkKind::from(node.network.as_str()),
                    header_opt,
                )?),
                ChainCode::Litecoin => Arc::new(LtcTx::new(&node.rpc_url, header_opt)?),
                ChainCode::Dogcoin => Arc::new(DogeTx::new(&node.rpc_url, header_opt)?),
                ChainCode::Sui => Arc::new(SuiTx::new(&node.rpc_url, header_opt)?),
                ChainCode::Ton => Arc::new(TonTx::new(&node.rpc_url, header_opt)?),
            };

            // 存入缓存
            self.transaction_adapter.insert(cache_key, adapter.clone());
        }

        tracing::info!("所有链和节点的适配器预初始化完成");
        Ok(())
    }

    pub async fn new_transaction_adapter(
        &self,
        chain_code: ChainCode,
    ) -> Result<Arc<dyn Tx + Send + Sync>, ServiceError> {
        // 首先获取链节点信息
        let node = Self::get_chain_node(chain_code).await?;

        // 使用chain_code和rpc_url组合作为缓存键，确保不同节点的适配器不会被混用
        let cache_key = format!("{}:{}", chain_code.to_string(), node.rpc_url);

        // 尝试从缓存获取
        if let Some(adapter) = self.transaction_adapter.get(&cache_key) {
            tracing::info!(chain_code=%chain_code, rpc_url=%node.rpc_url, "使用缓存的transaction_adapter");
            return Ok(adapter.clone());
        }

        // 缓存未命中，创建新的适配器
        tracing::info!(rpc_url=%node.rpc_url, chain_code=%chain_code, "创建新的transaction_adapter");
        let header_opt = if rpc_need_header(&node.rpc_url)? {
            Some(crate::context::CONTEXT.get().unwrap().get_rpc_header().await?)
        } else {
            None
        };

        let adapter: Arc<dyn Tx + Send + Sync> = match chain_code {
            ChainCode::Tron => Arc::new(TronTx::new(&node.rpc_url, header_opt)?),
            ChainCode::Bitcoin => Arc::new(BtcTx::new(&node.rpc_url, header_opt)?),
            ChainCode::Solana => Arc::new(SolTx::new(&node.rpc_url, header_opt)?),
            ChainCode::Ethereum => Arc::new(EthTx::new(
                chain_code,
                &node.rpc_url,
                NetworkKind::from(node.network.as_str()),
                header_opt,
            )?),
            ChainCode::BnbSmartChain => Arc::new(EthTx::new(
                chain_code,
                &node.rpc_url,
                NetworkKind::from(node.network.as_str()),
                header_opt,
            )?),
            ChainCode::Litecoin => Arc::new(LtcTx::new(&node.rpc_url, header_opt)?),
            ChainCode::Dogcoin => Arc::new(DogeTx::new(&node.rpc_url, header_opt)?),
            ChainCode::Sui => Arc::new(SuiTx::new(&node.rpc_url, header_opt)?),
            ChainCode::Ton => Arc::new(TonTx::new(&node.rpc_url, header_opt)?),
        };

        // 存入缓存
        self.transaction_adapter.insert(cache_key, adapter.clone());
        Ok(adapter)
    }

    /// 静态方法，内部调用全局单例
    pub async fn get_transaction_adapter(
        chain_code: ChainCode,
    ) -> Result<Arc<dyn Tx + Send + Sync>, ServiceError> {
        let factory = Self::get_instance();
        factory.new_transaction_adapter(chain_code).await
    }
}
