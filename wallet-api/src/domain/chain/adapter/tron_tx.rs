use crate::{
    domain::{
        chain::swap::{evm_swap::dexSwap1Call, EstimateSwapResult, TRON_SWAP_ADDRESS},
        multisig::MultisigQueueDomain,
    },
    request::transaction::ApproveParams,
    response_vo::TransferParams,
};
use alloy::{primitives::U256, sol_types::SolCall};
use wallet_chain_interact::{
    tron::{
        operations::{
            contract::{TriggerContractParameter, WarpContract},
            transfer::{ContractTransferOpt, TransferOpt},
            trc::{Allowance, Approve},
            TronConstantOperation as _,
        },
        TronChain,
    },
    types::{ChainPrivateKey, MultisigTxResp},
};

// 构建多签交易
pub(super) async fn build_build_tx(
    req: &TransferParams,
    token: Option<String>,
    value: alloy::primitives::U256,
    chain: &TronChain,
    threshold: i64,
    permission_id: Option<i64>,
) -> Result<MultisigTxResp, crate::ServiceError> {
    let expiration = MultisigQueueDomain::sub_expiration(req.expiration.unwrap_or(1));

    if let Some(token) = token {
        let mut params =
            ContractTransferOpt::new(&token, &req.from, &req.to, value, req.notes.clone())?;

        params.permission_id = permission_id;

        let provider = chain.get_provider();
        let constant = params.constant_contract(provider).await?;
        let consumer = provider
            .contract_fee(constant, threshold as u8, &req.from)
            .await?;
        params.set_fee_limit(consumer);

        Ok(chain
            .build_multisig_transaction(params, expiration as u64)
            .await?)
    } else {
        let mut params = TransferOpt::new(&req.from, &req.to, value, req.notes.clone())?;
        params.permission_id = permission_id;

        Ok(chain
            .build_multisig_transaction(params, expiration as u64)
            .await?)
    }
}

pub(super) async fn approve(
    chain: &TronChain,
    req: &ApproveParams,
    value: alloy::primitives::U256,
    key: ChainPrivateKey,
) -> Result<String, crate::ServiceError> {
    let approve = Approve::new(&req.from, &req.spender, &req.contract, value);
    let mut wrap = WarpContract::new(approve)?;

    // get fee
    let constant = wrap.trigger_constant_contract(&chain.provider).await?;
    let consumer = chain.provider.contract_fee(constant, 1, &req.from).await?;

    // exec tx
    let raw_transaction = wrap
        .trigger_smart_contract(&chain.provider, &consumer)
        .await?;

    let result = chain.exec_transaction_v1(raw_transaction, key).await?;
    Ok(result)
}

pub(super) async fn allowance(
    chain: &TronChain,
    from: &str,
    token: &str,
) -> Result<U256, crate::ServiceError> {
    let approve = Allowance::new(from, token, TRON_SWAP_ADDRESS);
    let wrap = WarpContract::new(approve)?;

    // get fee
    let constant = wrap.trigger_constant_contract(&chain.provider).await?;

    Ok(constant.parse_u256()?)
}

pub(super) async fn estimate_swap(
    call_value: dexSwap1Call,
    chain: &TronChain,
    recipient: &str,
) -> Result<EstimateSwapResult, crate::ServiceError> {
    // 构建调用合约的参数
    let parameter = wallet_utils::hex_func::hex_encode(call_value.abi_encode());
    let value = TriggerContractParameter::new(TRON_SWAP_ADDRESS, recipient, "", parameter);

    let wrap = WarpContract { params: value };
    let constant = wrap.trigger_constant_contract(&chain.provider).await?;

    // 模拟的结果
    let amount_in = constant.parse_num(&constant.constant_result[0])?;
    let amount_out = constant.parse_num(&constant.constant_result[1])?;

    // get fee
    let consumer = chain.provider.contract_fee(constant, 1, recipient).await?;

    let resp = EstimateSwapResult {
        amount_in,
        amount_out,
        fee: alloy::primitives::U256::from(consumer.transaction_fee_i64()),
        consumer: wallet_utils::serde_func::serde_to_string(&consumer)?,
    };

    Ok(resp)
}

// 执行swap操作
pub(super) async fn swap(
    call_value: dexSwap1Call,
    chain: &TronChain,
    recipient: &str,
    key: ChainPrivateKey,
) -> Result<String, crate::ServiceError> {
    // 构建调用合约的参数
    let parameter = wallet_utils::hex_func::hex_encode(call_value.abi_encode());
    let value = TriggerContractParameter::new(TRON_SWAP_ADDRESS, recipient, "", parameter);

    let mut wrap = WarpContract { params: value };
    let constant = wrap.trigger_constant_contract(&chain.provider).await?;
    // get fee
    let consumer = chain.provider.contract_fee(constant, 1, recipient).await?;

    let raw_transaction = wrap
        .trigger_smart_contract(&chain.provider, &consumer)
        .await?;

    let hash = chain.exec_transaction_v1(raw_transaction, key).await?;

    Ok(hash)
}
