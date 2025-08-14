mod transaction_adapter;
pub use transaction_adapter::*;
use wallet_chain_interact::{
    tron::{self, TronChain},
    types::ChainPrivateKey,
};
use wallet_transport::client::HttpClient;
use wallet_types::constant::chain_code;
mod multisig_adapter;
use crate::request::transaction::TransferReq;

use super::rpc_need_header;
pub use multisig_adapter::*;
pub mod eth;
pub mod eth_tx;
pub mod ton_tx;
pub mod tron_tx;

use wallet_database::entities::chain::{ChainEntity, ChainWithNode};

const TIME_OUT: u64 = 30;

#[macro_export]
macro_rules! dispatch {
    ($self:expr, $method:ident, $($arg:expr),*) => {
        match $self {
            Self::BitCoin(chain) => chain.$method($($arg),*).await,
            Self::Ethereum(chain) => chain.$method($($arg),*).await,
            Self::Solana(chain) => chain.$method($($arg),*).await,
            Self::Tron(chain) => chain.$method($($arg),*).await,
            Self::Ltc(chain) => chain.$method($($arg),*).await,
            Self::Doge(chain) => chain.$method($($arg),*).await,
            Self::Ton(chain) => chain.$method($($arg),*).await,
            Self::Sui(chain) => chain.$method($($arg),*).await,
        }
    };
}

pub struct ChainAdapterFactory;
impl ChainAdapterFactory {
    async fn get_chain_node(chain_code: &str) -> Result<ChainWithNode, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let node = ChainEntity::chain_node_info(pool.as_ref(), chain_code)
            .await?
            .ok_or(crate::BusinessError::Chain(crate::ChainError::NotFound(
                chain_code.to_string(),
            )))?;
        Ok(node)
    }

    pub async fn get_multisig_adapter(
        chain_code: &str,
    ) -> Result<MultisigAdapter, crate::ServiceError> {
        let node = ChainAdapterFactory::get_chain_node(chain_code).await?;

        let chain = wallet_types::chain::chain::ChainCode::try_from(node.chain_code.as_str())?;

        let header_opt = if rpc_need_header(&node.rpc_url)? {
            Some(crate::Context::get_rpc_header().await?)
        } else {
            None
        };

        MultisigAdapter::new(chain, node, header_opt)
    }

    pub async fn get_transaction_adapter(
        chain_code: &str,
    ) -> Result<TransactionAdapter, crate::ServiceError> {
        let node = ChainAdapterFactory::get_chain_node(chain_code).await?;
        let chain = wallet_types::chain::chain::ChainCode::try_from(node.chain_code.as_str())?;

        let header_opt = if rpc_need_header(&node.rpc_url)? {
            Some(crate::Context::get_rpc_header().await?)
        } else {
            None
        };

        Ok(TransactionAdapter::new(chain, &node.rpc_url, header_opt)?)
    }

    pub async fn get_tron_adapter() -> Result<TronChain, crate::ServiceError> {
        let node = ChainAdapterFactory::get_chain_node(chain_code::TRON).await?;

        let header_opt = if rpc_need_header(&node.rpc_url)? {
            Some(crate::Context::get_rpc_header().await?)
        } else {
            None
        };
        let timeout = Some(std::time::Duration::from_secs(TIME_OUT));

        let http_client = HttpClient::new(&node.rpc_url, header_opt, timeout)?;
        let provider = tron::Provider::new(http_client)?;

        Ok(tron::TronChain::new(provider)?)
    }

    pub async fn get_node_transaction_adapter(
        chain_code: &str,
        rpc_url: &str,
    ) -> Result<TransactionAdapter, crate::ServiceError> {
        let chain = wallet_types::chain::chain::ChainCode::try_from(chain_code)?;

        let header_opt = if rpc_need_header(rpc_url)? {
            Some(crate::Context::get_rpc_header().await?)
        } else {
            None
        };

        Ok(TransactionAdapter::new(chain, rpc_url, header_opt)?)
    }
}

// transfer estimate fee t
#[async_trait::async_trait]
pub trait ChainAction {
    type Provider;
    type FeeInfo;

    // 获取 gas 估算
    async fn estimate_fee(
        &self,
        provider: Self::FeeInfo,
    ) -> Result<Self::FeeInfo, crate::ServiceError>;

    // 执行操作（包含签名、广播）
    async fn execute(
        &self,
        fee: String,
        key: ChainPrivateKey,
    ) -> Result<TransferReq, crate::ServiceError>;
}
