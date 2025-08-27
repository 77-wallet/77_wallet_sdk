use crate::{
    domain::{
        api_wallet::adapter::{
            btc_tx::BtcTx, doge_tx::DogeTx, eth_tx::EthTx, ltx_tx::LtcTx, sol_tx::SolTx, sui_tx::SuiTx,
            ton_tx::TonTx, tron_tx::TronTx, Tx,
        },
        chain::rpc_need_header,
    }, BusinessError, Context,
    ServiceError,
};
use dashmap::DashMap;
use std::sync::Arc;
use wallet_database::entities::chain::{ChainEntity, ChainWithNode};
use wallet_types::chain::{chain::ChainCode, network::NetworkKind};

pub(crate) static API_ADAPTER_FACTORY: once_cell::sync::Lazy<
    tokio::sync::OnceCell<ApiChainAdapterFactory>,
> = once_cell::sync::Lazy::new(tokio::sync::OnceCell::new);

pub struct ApiChainAdapterFactory {
    transaction_adapter: DashMap<String, Arc<dyn Tx + Send + Sync>>,
}

impl ApiChainAdapterFactory {
    pub async fn new() -> Result<ApiChainAdapterFactory, ServiceError> {
        let adapter = DashMap::new();
        adapter.insert(
            ChainCode::Bitcoin.to_string(),
            Self::new_transaction_adapter(ChainCode::Bitcoin).await?,
        );
        adapter.insert(
            ChainCode::Dogcoin.to_string(),
            Self::new_transaction_adapter(ChainCode::Dogcoin).await?,
        );
        adapter.insert(
            ChainCode::Ethereum.to_string(),
            Self::new_transaction_adapter(ChainCode::Ethereum).await?,
        );
        adapter.insert(
            ChainCode::Litecoin.to_string(),
            Self::new_transaction_adapter(ChainCode::Litecoin).await?,
        );
        adapter.insert(
            ChainCode::Solana.to_string(),
            Self::new_transaction_adapter(ChainCode::Solana).await?,
        );
        adapter.insert(
            ChainCode::Sui.to_string(),
            Self::new_transaction_adapter(ChainCode::Sui).await?,
        );
        adapter.insert(
            ChainCode::Ton.to_string(),
            Self::new_transaction_adapter(ChainCode::Ton).await?,
        );
        adapter.insert(
            ChainCode::Tron.to_string(),
            Self::new_transaction_adapter(ChainCode::Tron).await?,
        );
        adapter.insert(
            ChainCode::BnbSmartChain.to_string(),
            Self::new_transaction_adapter(ChainCode::BnbSmartChain).await?,
        );

        Ok(ApiChainAdapterFactory { transaction_adapter: adapter })
    }

    async fn get_chain_node(chain_code: ChainCode) -> Result<ChainWithNode, crate::ServiceError> {
        let pool = Context::get_global_sqlite_pool()?;
        let node = ChainEntity::chain_node_info(pool.as_ref(), chain_code.to_string().as_str())
            .await?
            .ok_or(crate::BusinessError::Chain(crate::ChainError::NotFound(
                chain_code.to_string(),
            )))?;
        Ok(node)
    }

    async fn new_transaction_adapter(
        chain_code: ChainCode,
    ) -> Result<Arc<dyn Tx + Send + Sync>, crate::ServiceError> {
        let node = Self::get_chain_node(chain_code).await?;
        tracing::info!(rpc_url=%node.rpc_url, "new_transaction_adapter");
        let header_opt = if rpc_need_header(&node.rpc_url)? {
            Some(crate::Context::get_rpc_header().await?)
        } else {
            None
        };
        match chain_code {
            ChainCode::Tron => Ok(Arc::new(TronTx::new(&node.rpc_url, header_opt)?)),
            ChainCode::Bitcoin => Ok(Arc::new(BtcTx::new(&node.rpc_url, header_opt)?)),
            ChainCode::Solana => Ok(Arc::new(SolTx::new(&node.rpc_url, header_opt)?)),
            ChainCode::Ethereum => Ok(Arc::new(EthTx::new(
                chain_code,
                &node.rpc_url,
                NetworkKind::from(node.network.as_str()),
                header_opt,
            )?)),
            ChainCode::BnbSmartChain => Ok(Arc::new(EthTx::new(
                chain_code,
                &node.rpc_url,
                NetworkKind::from(node.network.as_str()),
                header_opt,
            )?)),
            ChainCode::Litecoin => Ok(Arc::new(LtcTx::new(&node.rpc_url, header_opt)?)),
            ChainCode::Dogcoin => Ok(Arc::new(DogeTx::new(&node.rpc_url, header_opt)?)),
            ChainCode::Sui => Ok(Arc::new(SuiTx::new(&node.rpc_url, header_opt)?)),
            ChainCode::Ton => Ok(Arc::new(TonTx::new(&node.rpc_url, header_opt)?)),
        }
    }

    pub async fn get_transaction_adapter(
        &self,
        chain_code: &str,
    ) -> Result<Arc<dyn Tx + Send + Sync>, crate::ServiceError> {
        let res = self.transaction_adapter.get(chain_code);
        match res {
            Some(kv) => Ok(kv.value().clone()),
            _ => Err(crate::ServiceError::Business(BusinessError::Chain(
                crate::ChainError::NotFound(chain_code.to_owned()),
            ))),
        }
    }
}
