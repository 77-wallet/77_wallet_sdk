use crate::{
    domain::chain::{
        pare_fee_setting,
        swap::{
            evm_swap::{dexSwap1Call, SwapParams},
            EstimateSwapResult,
        },
        TransferResp,
    },
    request::transaction::{ApproveReq, DepositReq, WithdrawReq},
};
use alloy::{
    network::TransactionBuilder as _,
    sol_types::{SolCall as _, SolValue},
};
use alloy::{primitives::U256, rpc::types::TransactionRequest};
use wallet_chain_interact::{
    eth::{
        self,
        operations::erc::{Allowance, Approve, Deposit, Withdraw},
        EthChain, FeeSetting,
    },
    types::ChainPrivateKey,
    ResourceConsume,
};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::unit;

pub(super) async fn approve(
    chain: &EthChain,
    req: &ApproveReq,
    value: alloy::primitives::U256,
    key: ChainPrivateKey,
) -> Result<TransferResp, crate::ServiceError> {
    let approve = Approve::new(&req.from, &req.spender, value, &req.contract)?;

    // 使用默认的手续费配置
    let gas_price = chain.provider.gas_price().await?;
    let fee_setting = FeeSetting::new_with_price(gas_price);

    let fee = fee_setting.transaction_fee();

    // exec tx
    let tx_hash = chain.exec_transaction(approve, fee_setting, key).await?;

    Ok(TransferResp::new(
        tx_hash,
        unit::format_to_string(fee, eth::consts::ETH_DECIMAL)?,
    ))
}

pub(super) async fn approve_fee(
    chain: &EthChain,
    req: &ApproveReq,
    value: alloy::primitives::U256,
) -> Result<ResourceConsume, crate::ServiceError> {
    let approve = Approve::new(&req.from, &req.spender, value, &req.contract)?;

    let fee = chain.estimate_gas(approve).await?;

    Ok(fee)
}

pub(super) async fn withdraw_fee(
    chain: &EthChain,
    req: &WithdrawReq,
    value: alloy::primitives::U256,
) -> Result<ResourceConsume, crate::ServiceError> {
    let withdraw = Withdraw::new(&req.from, &req.token, value)?;

    Ok(chain.estimate_gas(withdraw).await?)
}

pub(super) async fn withdraw(
    chain: &EthChain,
    req: &WithdrawReq,
    value: alloy::primitives::U256,
    fee: String,
    key: ChainPrivateKey,
) -> Result<TransferResp, crate::ServiceError> {
    let withdraw = Withdraw::new(&req.from, &req.token, value)?;

    // 使用默认的手续费配置
    let fee_setting = pare_fee_setting(fee.as_str())?;
    let transfer_fee = fee_setting.transaction_fee();

    // exec tx
    let tx_hash = chain.exec_transaction(withdraw, fee_setting, key).await?;

    Ok(TransferResp::new(
        tx_hash,
        unit::format_to_string(transfer_fee, eth::consts::ETH_DECIMAL)?,
    ))
}

pub(super) async fn deposit_fee(
    chain: &EthChain,
    req: &DepositReq,
    amount: alloy::primitives::U256,
) -> Result<ResourceConsume, crate::ServiceError> {
    let approve = Deposit::new(&req.from, &req.token, amount)?;

    Ok(chain.estimate_gas(approve).await?)
}

pub(super) async fn deposit(
    chain: &EthChain,
    req: &DepositReq,
    amount: alloy::primitives::U256,
    fee: String,
    key: ChainPrivateKey,
) -> Result<TransferResp, crate::ServiceError> {
    let approve = Deposit::new(&req.from, &req.token, amount)?;

    // 使用默认的手续费配置
    let fee_setting = pare_fee_setting(fee.as_str())?;
    let transfer_fee = fee_setting.transaction_fee();

    // exec tx
    let tx_hash = chain.exec_transaction(approve, fee_setting, key).await?;

    Ok(TransferResp::new(
        tx_hash,
        unit::format_to_string(transfer_fee, eth::consts::ETH_DECIMAL)?,
    ))
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

fn build_base_swap_tx(swap_params: &SwapParams) -> Result<TransactionRequest, crate::ServiceError> {
    let call_value = dexSwap1Call::try_from((swap_params, ChainCode::Ethereum))?;

    let tx = TransactionRequest::default()
        .from(swap_params.recipient)
        .to(swap_params.aggregator_addr)
        .with_input(call_value.abi_encode());

    // from token 如果是主币添加默认的币
    let tx = if swap_params.token_in.is_zero() {
        tx.with_value(swap_params.amount_in)
    } else {
        tx
    };

    Ok(tx)
}
// 有3个网络的io
pub(super) async fn estimate_swap(
    swap_params: SwapParams,
    chain: &EthChain,
) -> Result<EstimateSwapResult<FeeSetting>, crate::ServiceError> {
    let tx = build_base_swap_tx(&swap_params)?;

    // estimate_fee
    let gas_limit = chain.provider.estimate_gas(tx.clone()).await?;
    let tx = tx.with_gas_limit(gas_limit.to::<u64>());

    let gas_price = chain.provider.gas_price().await?;
    let mut fee = FeeSetting::new_with_price(gas_price);
    fee.gas_limit = gas_limit;

    let result = chain.provider.eth_call(tx).await?;
    let bytes = wallet_utils::hex_func::hex_decode(&result[2..])?;

    let (amount_in, amount_out): (U256, U256) = <(U256, U256)>::abi_decode_params(&bytes, true)
        .map_err(|e| crate::ServiceError::AggregatorError {
            code: -1,
            agg_code: 0,
            msg: e.to_string(),
        })?;

    let resp = EstimateSwapResult {
        amount_in,
        amount_out,
        consumer: fee,
    };
    Ok(resp)
}

pub(super) async fn swap(
    chain: &EthChain,
    swap_params: &SwapParams,
    fee: String,
    key: ChainPrivateKey,
) -> Result<TransferResp, crate::ServiceError> {
    let fee = pare_fee_setting(fee.as_str())?;
    let transfer_fee = fee.transaction_fee();

    let tx = build_base_swap_tx(&swap_params)?;
    let tx = chain
        .provider
        .set_transaction_fee(tx, fee, chain.chain_code)
        .await?;

    let tx_hash = chain.provider.send_raw_transaction(tx, &key).await?;

    Ok(TransferResp::new(
        tx_hash,
        unit::format_to_string(transfer_fee, eth::consts::ETH_DECIMAL)?,
    ))
}
