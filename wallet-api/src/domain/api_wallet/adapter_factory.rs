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
use std::sync::Arc;
use wallet_database::entities::chain::{ChainEntity, ChainWithNode};
use wallet_types::chain::{chain::ChainCode, network::NetworkKind};

pub struct ApiChainAdapterFactory {
    transaction_adapter: DashMap<String, Arc<dyn Tx + Send + Sync>>,
}

impl ApiChainAdapterFactory {
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

    pub async fn new_transaction_adapter(
        chain_code: ChainCode,
    ) -> Result<Arc<dyn Tx + Send + Sync>, ServiceError> {
        let node = Self::get_chain_node(chain_code).await?;
        tracing::info!(rpc_url=%node.rpc_url, "new_transaction_adapter");
        let header_opt = if rpc_need_header(&node.rpc_url)? {
            Some(crate::context::CONTEXT.get().unwrap().get_rpc_header().await?)
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
}
