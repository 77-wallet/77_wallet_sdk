use crate::{
    domain::{
        api_wallet::{account::ApiAccountDomain, adapter_factory::ApiChainAdapterFactory},
        chain::TransferResp,
    },
    error::service::ServiceError,
    request::api_wallet::trans::ApiTransferReq,
};
use wallet_types::chain::chain::ChainCode;

pub(crate) mod collect;
pub(crate) mod fee;
pub(crate) mod withdraw;

pub(crate) struct ApiTransDomain {}

impl ApiTransDomain {
    /// transfer
    pub async fn transfer(params: ApiTransferReq) -> Result<TransferResp, ServiceError> {
        tracing::info!("transfer ------------------- 7:");
        let private_key = ApiAccountDomain::get_private_key(
            &params.base.from,
            &params.base.chain_code,
            &params.password,
        )
        .await?;

        tracing::info!("transfer ------------------- 8: {}", params.base.chain_code);

        let chain_code: ChainCode = params.base.chain_code.as_str().try_into()?;
        tracing::info!("transfer ------------------- 9: {}", chain_code);
        let adapter = ApiChainAdapterFactory::new_transaction_adapter(chain_code).await?;

        tracing::info!("transfer ------------------- 10:");

        let resp = adapter.transfer(&params, private_key).await?;

        tracing::info!("transfer ------------------- 11:");

        if let Some(request_id) = params.base.request_resource_id {
            let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            let _ = backend.delegate_complete(&request_id).await;
        }

        Ok(resp)
    }

    pub async fn nonce(from_addr: &str, chain_code: &str) -> Result<u64, ServiceError> {
        let chain_code: ChainCode = chain_code.try_into()?;
        let adapter = ApiChainAdapterFactory::new_transaction_adapter(chain_code).await?;
        adapter.nonce(from_addr).await
    }
}
