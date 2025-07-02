use crate::{
    domain::chain::{
        pare_fee_setting,
        swap::{evm_swap::dexSwap1Call, EstimateSwapResult, ETH_SWAP_ADDRESS},
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
) -> Result<U256, crate::ServiceError> {
    let approve = Allowance::new(from, token, ETH_SWAP_ADDRESS)?;

    let amount = chain.eth_call::<_, U256>(approve).await?;

    Ok(amount)
}

pub(super) async fn estimate_swap(
    call_value: dexSwap1Call,
    chain: &EthChain,
    recipient: &str,
) -> Result<EstimateSwapResult, crate::ServiceError> {
    let tx = TransactionRequest::default()
        .from(wallet_utils::address::parse_eth_address(recipient)?)
        .to(wallet_utils::address::parse_eth_address(ETH_SWAP_ADDRESS)?)
        .with_input(call_value.abi_encode());

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
    call_value: dexSwap1Call,
    chain: &EthChain,
    recipient: &str,
    fee: String,
    key: ChainPrivateKey,
) -> Result<String, crate::ServiceError> {
    let fee = pare_fee_setting(fee.as_str())?;

    // 构建交易
    let tx = TransactionRequest::default()
        .from(wallet_utils::address::parse_eth_address(recipient)?)
        .to(wallet_utils::address::parse_eth_address(ETH_SWAP_ADDRESS)?)
        .with_input(call_value.abi_encode());

    let tx = chain
        .provider
        .set_transaction_fee(tx, fee, chain.chain_code)
        .await?;

    let hash = chain.provider.send_raw_transaction(tx, &key).await?;

    Ok(hash)
}
