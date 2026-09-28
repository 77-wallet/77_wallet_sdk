mod scroll;

use super::ChainAdapterFactory;
use crate::{
    context::Context, domain::chain::transaction::ChainTransDomain, error::service::ServiceError,
    request::transaction::BaseTransferReq, response_vo::FeeDetails,
};
use wallet_chain_interact::{
    eth::{EthChain, operations::TransferOpt},
    types::Transaction as _,
};
use wallet_transport_backend::{api::BackendApi, response_vo::coin::TokenCurrency};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::unit;

#[derive(Debug, PartialEq, Eq)]
enum AdditionalFeePolicy {
    None,
    ScrollRpc,
}

fn additional_fee_policy(chain_code: ChainCode) -> AdditionalFeePolicy {
    match chain_code {
        ChainCode::Scroll => AdditionalFeePolicy::ScrollRpc,
        _ => AdditionalFeePolicy::None,
    }
}

pub(super) async fn estimate_transfer_fee(
    ctx: &'static Context,
    chain: &EthChain,
    req: &BaseTransferReq,
    backend_api: &BackendApi,
    currency: &str,
    token_currency: TokenCurrency,
) -> Result<String, ServiceError> {
    let token_address = req.token_address.to_chain_token_option();
    let value = unit::convert_to_u256(&req.value, req.decimals)?;
    let balance = chain.balance(&req.from, token_address.clone()).await?;
    if balance < value {
        return Err(crate::error::business::BusinessError::Chain(
            crate::error::business::chain::ChainError::insufficient_balance_with_detail(
                crate::error::business::chain::InsufficientBalanceDetail::new()
                    .from_addr(req.from.clone())
                    .to_addr(req.to.clone())
                    .chain_code(req.chain_code.clone())
                    .token_addr(token_address.clone().unwrap_or_default())
                    .value(value.to_string())
                    .balance(balance.to_string())
                    .need(value.to_string())
                    .reason("token balance is insufficient for fee estimation"),
            ),
        ))?;
    }

    let gas_oracle =
        ChainTransDomain::gas_oracle(&req.chain_code, &chain.provider, backend_api).await?;
    let params = TransferOpt::new(&req.from, &req.to, value, token_address)?;
    let additional_fee = match additional_fee_policy(chain.chain_code) {
        AdditionalFeePolicy::None => None,
        AdditionalFeePolicy::ScrollRpc => {
            let tx = params.build_transaction()?;
            let node = ChainAdapterFactory::get_chain_node_with_ctx(ctx, &req.chain_code).await?;
            let headers = if super::rpc_need_header(&node.rpc_url)? {
                Some(ctx.get_rpc_header().await?)
            } else {
                None
            };
            Some(scroll::estimate_l1_data_fee(&node.rpc_url, headers, tx).await?)
        }
    };
    let gas = chain.estimate_gas(params).await?;
    let fee = FeeDetails::try_from((gas_oracle, gas.consume))?;
    let fee = match additional_fee {
        Some(additional_fee) => fee.with_additional_native_fee(additional_fee)?,
        None => fee,
    };
    Ok(wallet_utils::serde_func::serde_to_string(&fee.to_resp(token_currency, currency))?)
}

#[cfg(test)]
mod tests;
