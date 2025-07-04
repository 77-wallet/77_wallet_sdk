use crate::{
    domain::chain::{
        pare_fee_setting,
        swap::{
            evm_swap::{dexSwap1Call, SwapParams},
            EstimateSwapResult,
        },
    },
    request::transaction::{ApproveParams, DepositParams},
};
use alloy::{
    network::TransactionBuilder as _,
    sol_types::{SolCall as _, SolValue},
};
use alloy::{primitives::U256, rpc::types::TransactionRequest};
use wallet_chain_interact::{
    eth::{
        operations::erc::{Allowance, Approve, Deposit},
        EthChain, FeeSetting,
    },
    types::ChainPrivateKey,
};

pub(super) async fn approve(
    chain: &EthChain,
    req: &ApproveParams,
    value: alloy::primitives::U256,
    key: ChainPrivateKey,
) -> Result<String, crate::ServiceError> {
    let approve = Approve::new(&req.from, &req.spender, value, &req.contract)?;

    // 使用默认的手续费配置
    let gas_price = chain.provider.gas_price().await?;
    let fee_setting = FeeSetting::new_with_price(gas_price);

    // exec tx
    let hash = chain.exec_transaction(approve, fee_setting, key).await?;

    Ok(hash)
}

// deposit 获取币
pub(super) async fn deposit(
    chain: &EthChain,
    req: &DepositParams,
    value: alloy::primitives::U256,
    key: ChainPrivateKey,
) -> Result<String, crate::ServiceError> {
    let deposit = Deposit::new(&req.from, &req.contract, value)?;

    // 使用默认的手续费配置
    let gas_price = chain.provider.gas_price().await?;
    let fee_setting = FeeSetting::new_with_price(gas_price);

    // exec tx
    let hash = chain.exec_transaction(deposit, fee_setting, key).await?;

    Ok(hash)
}

pub(super) async fn allowance(
    chain: &EthChain,
    from: &str,
    token: &str,
    spender: &str,
) -> Result<U256, crate::ServiceError> {
    let approve = Allowance::new(from, token, spender)?;

    let amount = chain.eth_call::<_, U256>(approve).await?;

    Ok(amount)
}

pub(super) async fn estimate_swap(
    swap_params: SwapParams,
    chain: &EthChain,
) -> Result<EstimateSwapResult, crate::ServiceError> {
    let call_value = dexSwap1Call::try_from(&swap_params)?;

    let tx = TransactionRequest::default()
        .from(swap_params.recipient)
        .to(swap_params.aggregator_addr)
        .with_input(call_value.abi_encode());

    let tx = if swap_params.token_in.is_zero() {
        tx.with_value(swap_params.amount_in)
    } else {
        tx
    };

    // estimate_fee
    let gas_limit = chain.provider.estimate_gas(tx.clone()).await?;
    let tx = tx.with_gas_limit(gas_limit.to::<u64>());

    let gas_price = chain.provider.gas_price().await?;
    let mut fee = FeeSetting::new_with_price(gas_price);
    fee.gas_limit = gas_limit;

    let result = chain.provider.eth_call(tx).await?;
    let bytes = wallet_utils::hex_func::hex_decode(&result[2..])?;

    let (amount_in, amount_out): (U256, U256) = <(U256, U256)>::abi_decode_params(&bytes, true)
        .map_err(|e| crate::ServiceError::AggregatorError(e.to_string()))?;

    let fee = fee.transaction_fee();
    let consumer = wallet_utils::serde_func::serde_to_string(&fee)?;

    let resp = EstimateSwapResult {
        amount_in,
        amount_out,
        fee,
        consumer,
    };
    Ok(resp)
}

pub(super) async fn swap(
    chain: &EthChain,
    swap_params: &SwapParams,
    fee: String,
    key: ChainPrivateKey,
) -> Result<String, crate::ServiceError> {
    let fee = pare_fee_setting(fee.as_str())?;

    let call_value = dexSwap1Call::try_from(swap_params)?;
    // 构建交易
    let tx = TransactionRequest::default()
        .from(swap_params.recipient)
        .to(swap_params.aggregator_addr)
        .with_input(call_value.abi_encode());

    let tx = if swap_params.token_in.is_zero() {
        tx.with_value(swap_params.amount_in)
    } else {
        tx
    };

    let tx = chain
        .provider
        .set_transaction_fee(tx, fee, chain.chain_code)
        .await?;

    let hash = chain.provider.send_raw_transaction(tx, &key).await?;

    Ok(hash)
}
