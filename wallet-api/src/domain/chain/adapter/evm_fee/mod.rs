use crate::{
    domain::chain::transaction::ChainTransDomain, error::service::ServiceError,
    request::transaction::BaseTransferReq, response_vo::FeeDetails,
};
use alloy::primitives::U256;
use wallet_chain_interact::{
    eth::{EthChain, FeeSetting, operations::TransferOpt},
    types::Transaction as _,
};
use wallet_transport_backend::{api::BackendApi, response_vo::coin::TokenCurrency};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::unit;

#[cfg(test)]
mod flow_tests;

#[cfg(all(test, feature = "smoke-live"))]
#[path = "../../../../../tests/smoke/transactions/evm_fee_live.rs"]
mod live_smoke_tests;

pub(super) fn execution_fee_reserve(chain_code: ChainCode, fee: &FeeSetting) -> U256 {
    match chain_code {
        ChainCode::BnbSmartChain => fee.transaction_fee(),
        _ => fee.gas_limit * fee.max_fee_per_gas,
    }
}

pub(super) async fn required_send_fee(
    chain: &EthChain,
    transfer: &TransferOpt,
    execution_fee: U256,
) -> Result<U256, ServiceError> {
    let additional_fee = chain.estimate_additional_fee(transfer.build_transaction()?).await?;
    Ok(execution_fee + additional_fee.unwrap_or_default())
}

pub(super) fn ensure_send_fee_balance(
    available_native: U256,
    required_fee: U256,
) -> Result<(), ServiceError> {
    if available_native < required_fee {
        return Err(crate::error::business::BusinessError::Chain(
            crate::error::business::chain::ChainError::InsufficientFeeBalance,
        ))?;
    }
    Ok(())
}

pub(super) async fn estimate_transfer_fee(
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
    let additional_fee = chain.estimate_additional_fee(params.build_transaction()?).await?;
    let gas = chain.estimate_gas(params).await?;
    let fee = FeeDetails::try_from((gas_oracle, gas.consume))?;
    let fee = match additional_fee {
        Some(additional_fee) => fee.with_additional_native_fee(additional_fee)?,
        None => fee,
    };
    Ok(wallet_utils::serde_func::serde_to_string(&fee.to_resp(token_currency, currency))?)
}
