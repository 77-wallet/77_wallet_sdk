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
    if chain.chain_code == ChainCode::Scroll {
        tracing::info!(target: "scroll_diagnostic", stage = "send_preflight",
            from = %transfer.base.from, to = %transfer.base.to, token = ?transfer.contract,
            value_wei = %transfer.base.value, execution_reserve_wei = %execution_fee,
            estimated_l1_fee_wei = %additional_fee.unwrap_or_default(),
            required_fee_wei = %(execution_fee + additional_fee.unwrap_or_default()),
            "Scroll diagnostic");
    }
    Ok(execution_fee + additional_fee.unwrap_or_default())
}

/// Reserve the maximum execution fee plus additional fees for the final value.
/// Keep the largest reservation seen so fluctuating estimates cannot release it.
pub(super) async fn prepare_native_maximum(
    chain: &EthChain,
    transfer: &mut TransferOpt,
    balance: U256,
    fee_setting: &mut FeeSetting,
    is_contract: bool,
) -> Result<U256, ServiceError> {
    let mut reserve = execution_fee_reserve(chain.chain_code, fee_setting);
    for _ in 0..4 {
        if balance <= reserve {
            return Err(crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::InsufficientFeeBalance,
            ))?;
        }
        let amount = balance - reserve;
        transfer.base.value = amount;
        if is_contract {
            // Payable contracts can consume different gas for different values.
            let gas = chain.provider.estimate_gas(transfer.build_transaction()?).await?;
            fee_setting.gas_limit = fee_setting.gas_limit.max(gas);
        }
        let execution_reserve = execution_fee_reserve(chain.chain_code, fee_setting);
        let required = required_send_fee(chain, transfer, execution_reserve).await?;
        if required <= reserve {
            return Ok(amount);
        }
        reserve = required;
    }
    // No signing or broadcast until a final-amount estimate fits the reservation.
    Err(crate::error::business::BusinessError::Chain(
        crate::error::business::chain::ChainError::InsufficientFeeBalance,
    ))?
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
    let value = if req.spend_all && token_address.is_none() { balance } else { value };
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
    if chain.chain_code == ChainCode::Scroll {
        tracing::info!(target: "scroll_diagnostic", stage = "fee_quote",
            from = %req.from, to = %req.to, token = ?req.token_address.to_chain_token_option(),
            value_wei = %value, spend_all = req.spend_all,
            estimated_l1_fee_wei = %additional_fee.unwrap_or_default(),
            "Scroll diagnostic");
    }
    let fee = match additional_fee {
        Some(additional_fee) => fee.with_additional_native_fee(additional_fee)?,
        None => fee,
    };
    Ok(wallet_utils::serde_func::serde_to_string(&fee.to_resp(token_currency, currency)?)?)
}
