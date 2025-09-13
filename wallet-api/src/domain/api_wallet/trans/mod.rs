use crate::{
    domain::{
        api_wallet::{
            account::ApiAccountDomain,
            adapter_factory::{API_ADAPTER_FACTORY, ApiChainAdapterFactory},
        },
        chain::TransferResp,
    },
    request::api_wallet::trans::ApiTransferReq,
};

pub(crate) mod collect;
pub(crate) mod fee;
pub(crate) mod withdraw;

pub(crate) struct ApiTransDomain {}

impl ApiTransDomain {
    /// transfer
    pub async fn transfer(params: ApiTransferReq) -> Result<TransferResp, crate::ServiceError> {
        tracing::info!("transfer fee ------------------- 7:");
        let private_key = ApiAccountDomain::get_private_key(
            &params.base.from,
            &params.base.chain_code,
            &params.password,
        )
        .await?;

        tracing::info!("transfer fee ------------------- 8:");

        let adapter = API_ADAPTER_FACTORY
            .get_or_init(|| async { ApiChainAdapterFactory::new().await.unwrap() })
            .await
            .get_transaction_adapter(params.base.chain_code.as_str())
            .await?;

        let resp = adapter.transfer(&params, private_key).await?;

        tracing::info!("transfer fee ------------------- 10:");

        if let Some(request_id) = params.base.request_resource_id {
            let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api()?;
            let _ = backend.delegate_complete(&request_id).await;
        }

        Ok(resp)
    }
}
