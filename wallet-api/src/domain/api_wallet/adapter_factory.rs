use crate::domain::{
    api_wallet::adapter::{
        btc_tx::BtcTx, doge_tx::DogeTx, eth_tx::EthTx, ltx_tx::LtcTx, sol_tx::SolTx, sui_tx::SuiTx,
        ton_tx::TonTx, tron_tx::TronTx, tx::ApiTxAdapter,
    },
    chain::rpc_need_header,
};
use crate::{BusinessError, Context};
use dashmap::DashMap;
use std::sync::Arc;
use wallet_database::entities::chain::{ChainEntity, ChainWithNode};
use wallet_types::chain::chain::ChainCode;

pub(crate) static API_ADAPTER_FACTORY: once_cell::sync::Lazy<
    tokio::sync::OnceCell<ApiChainAdapterFactory>,
> = once_cell::sync::Lazy::new(tokio::sync::OnceCell::new);

pub struct ApiChainAdapterFactory {
    transaction_adapter: DashMap<String, Arc<ApiTxAdapter>>,
}

impl ApiChainAdapterFactory {
    pub async fn new() -> ApiChainAdapterFactory {
        let adapter = DashMap::new();
        adapter.insert(
            ChainCode::Bitcoin.to_string(),
            Arc::new(
                Self::new_transaction_adapter(ChainCode::Bitcoin)
                    .await
                    .unwrap(),
            ),
        );
        adapter.insert(
            ChainCode::Dogcoin.to_string(),
            Arc::new(
                Self::new_transaction_adapter(ChainCode::Dogcoin)
                    .await
                    .unwrap(),
            ),
        );
        adapter.insert(
            ChainCode::Ethereum.to_string(),
            Arc::new(
                Self::new_transaction_adapter(ChainCode::Ethereum)
                    .await
                    .unwrap(),
            ),
        );
        adapter.insert(
            ChainCode::BnbSmartChain.to_string(),
            Arc::new(
                Self::new_transaction_adapter(ChainCode::BnbSmartChain)
                    .await
                    .unwrap(),
            ),
        );
        adapter.insert(
            ChainCode::Litecoin.to_string(),
            Arc::new(
                Self::new_transaction_adapter(ChainCode::Litecoin)
                    .await
                    .unwrap(),
            ),
        );
        adapter.insert(
            ChainCode::Solana.to_string(),
            Arc::new(
                Self::new_transaction_adapter(ChainCode::Solana)
                    .await
                    .unwrap(),
            ),
        );
        adapter.insert(
            ChainCode::Sui.to_string(),
            Arc::new(Self::new_transaction_adapter(ChainCode::Sui).await.unwrap()),
        );
        adapter.insert(
            ChainCode::Ton.to_string(),
            Arc::new(Self::new_transaction_adapter(ChainCode::Ton).await.unwrap()),
        );
        adapter.insert(
            ChainCode::Tron.to_string(),
            Arc::new(
                Self::new_transaction_adapter(ChainCode::Tron)
                    .await
                    .unwrap(),
            ),
        );

        ApiChainAdapterFactory {
            transaction_adapter: adapter,
        }
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
    ) -> Result<ApiTxAdapter, crate::ServiceError> {
        let node = Self::get_chain_node(chain_code).await?;
        let header_opt = if rpc_need_header(&node.rpc_url)? {
            Some(crate::Context::get_rpc_header().await?)
        } else {
            None
        };
        match chain_code {
            ChainCode::Tron => Ok(ApiTxAdapter::Tron(TronTx::new(&node.rpc_url, header_opt)?)),
            ChainCode::Bitcoin => Ok(ApiTxAdapter::Btc(BtcTx::new(&node.rpc_url, header_opt)?)),
            ChainCode::Solana => Ok(ApiTxAdapter::Sol(SolTx::new(&node.rpc_url, header_opt)?)),
            ChainCode::Ethereum => Ok(ApiTxAdapter::Eth(EthTx::new(
                chain_code,
                &node.rpc_url,
                header_opt,
            )?)),
            ChainCode::BnbSmartChain => Ok(ApiTxAdapter::Eth(EthTx::new(
                chain_code,
                &node.rpc_url,
                header_opt,
            )?)),
            ChainCode::Litecoin => Ok(ApiTxAdapter::Ltc(LtcTx::new(&node.rpc_url, header_opt)?)),
            ChainCode::Dogcoin => Ok(ApiTxAdapter::Doge(DogeTx::new(&node.rpc_url, header_opt)?)),
            ChainCode::Sui => Ok(ApiTxAdapter::Sui(SuiTx::new(&node.rpc_url, header_opt)?)),
            ChainCode::Ton => Ok(ApiTxAdapter::Ton(TonTx::new(&node.rpc_url, header_opt)?)),
        }
    }

    pub async fn get_transaction_adapter(
        &self,
        chain_code: &str,
    ) -> Result<Arc<ApiTxAdapter>, crate::ServiceError> {
        let res = self.transaction_adapter.get(chain_code);
        match res {
            Some(kv) => Ok(kv.value().clone()),
            _ => Err(crate::ServiceError::Business(BusinessError::Chain(
                crate::ChainError::NotFound(chain_code.to_owned()),
            ))),
        }
    }
}


#[async_trait::async_trait]
trait Tx {
    async fn send(&self, data: Vec<u8>);
}

struct MyTx;

#[async_trait::async_trait]
impl Tx for MyTx {
    async fn send(&self, data: Vec<u8>) {
        println!("Sending {:?}", data);
    }
}

#[tokio::main]
async fn main() {
    let tx: Box<dyn Tx + Send + Sync> = Box::new(MyTx);
    tx.send(vec![1, 2, 3]).await;
}

