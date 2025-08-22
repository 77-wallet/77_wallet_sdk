use crate::domain::api_wallet::adapter::{MultisigAdapter, TransactionAdapter};
use crate::domain::chain::rpc_need_header;
use wallet_chain_interact::tron;
use wallet_chain_interact::tron::TronChain;
use wallet_database::entities::chain::{ChainEntity, ChainWithNode};
use wallet_transport::client::HttpClient;
use wallet_types::chain::chain::ChainCode;
use wallet_types::constant::chain_code;

const TIME_OUT: u64 = 30;

pub struct ApiChainAdapterFactory;
impl ApiChainAdapterFactory {
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
        let node = Self::get_chain_node(chain_code).await?;

        let chain = ChainCode::try_from(node.chain_code.as_str())?;

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
        let node = Self::get_chain_node(chain_code).await?;
        let chain = ChainCode::try_from(node.chain_code.as_str())?;

        let header_opt = if rpc_need_header(&node.rpc_url)? {
            Some(crate::Context::get_rpc_header().await?)
        } else {
            None
        };

        Ok(TransactionAdapter::new(chain, &node.rpc_url, header_opt)?)
    }

    pub async fn get_tron_adapter() -> Result<TronChain, crate::ServiceError> {
        let node = Self::get_chain_node(chain_code::TRON).await?;

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
        let chain = ChainCode::try_from(chain_code)?;

        let header_opt = if rpc_need_header(rpc_url)? {
            Some(crate::Context::get_rpc_header().await?)
        } else {
            None
        };

        Ok(TransactionAdapter::new(chain, rpc_url, header_opt)?)
    }
}
