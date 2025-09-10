use crate::{
    domain::chain::{swap::EstimateSwapResult, TransferResp},
    request::transaction::{DepositReq, WithdrawReq},
};
use alloy::primitives::U256;
use wallet_chain_interact::{
    sol::{
        operations::{
            native_coin::{Deposit, Swap, Withdraw},
            SolInstructionOperation,
        },
        protocol::Instruction,
        SolFeeSetting, SolanaChain,
    },
    types::ChainPrivateKey,
};
use wallet_utils::serde_func;

pub(super) async fn estimate_swap(
    payer: &str,
    instructions: Vec<Instruction>,
    chain: &SolanaChain,
) -> Result<EstimateSwapResult<SolFeeSetting>, crate::ServiceError> {
    let payer = wallet_utils::address::parse_sol_address(payer)?;

    let _res = chain.similar_transaction(&payer, &instructions).await?;

    let fee_setting = SolFeeSetting {
        base_fee: 5000,
        priority_fee_per_compute_unit: None,
        compute_units_consumed: 10000,
        extra_fee: None,
    };

    let res = EstimateSwapResult {
        amount_in: U256::from(1),
        amount_out: U256::from(2),
        consumer: fee_setting,
    };

    Ok(res)
}

pub(super) async fn swap(
    chain: &SolanaChain,
    payer: &str,
    fee: String,
    instructions: Vec<Instruction>,
    key: ChainPrivateKey,
) -> Result<TransferResp, crate::ServiceError> {
    let fee_setting = wallet_utils::serde_func::serde_from_str::<SolFeeSetting>(&fee)?;
    let fee = fee_setting.transaction_fee().to_string();

    // check balance
    let balance = chain.balance(payer, None).await?;
    if U256::from(fee_setting.original_fee()) > balance {
        return Err(crate::BusinessError::Chain(
            crate::ChainError::InsufficientFeeBalance,
        ))?;
    }

    let params = Swap::new(payer)?;

    let tx_hash = chain
        .exec_transaction(params, key, Some(fee_setting), instructions, 0)
        .await?;

    let res = TransferResp {
        tx_hash,
        fee,
        consumer: None,
    };
    Ok(res)
}

pub(super) async fn deposit_fee(
    chain: &SolanaChain,
    req: &DepositReq,
    amount: alloy::primitives::U256,
) -> Result<SolFeeSetting, crate::ServiceError> {
    let params = Deposit::new_wsol(&req.from, amount, &chain.get_provider())?;

    let instructions = params.instructions().await?;

    Ok(chain.estimate_fee_v1(&instructions, &params).await?)
}

pub(super) async fn deposit(
    chain: &SolanaChain,
    req: &DepositReq,
    amount: alloy::primitives::U256,
    fee: String,
    key: ChainPrivateKey,
) -> Result<TransferResp, crate::ServiceError> {
    let fee_setting = serde_func::serde_from_str::<SolFeeSetting>(&fee)?;
    let fee = fee_setting.transaction_fee().to_string();
    let params = Deposit::new_wsol(&req.from, amount, &chain.get_provider())?;

    let instructions = params.instructions().await?;

    // check balance
    let balance = chain.balance(&req.from, None).await?;
    if U256::from(fee_setting.original_fee()) > balance {
        return Err(crate::BusinessError::Chain(
            crate::ChainError::InsufficientFeeBalance,
        ))?;
    }

    // 验证手续费,进行拦截
    let tx_hash = chain
        .exec_transaction(params, key, Some(fee_setting), instructions, 0)
        .await?;

    Ok(TransferResp {
        tx_hash,
        fee,
        consumer: None,
    })
}

pub(super) async fn withdraw_fee(
    chain: &SolanaChain,
    req: &WithdrawReq,
    amount: alloy::primitives::U256,
) -> Result<SolFeeSetting, crate::ServiceError> {
    let params = Withdraw::new_wsol(&req.from, amount, &chain.get_provider())?;

    let instructions = params.instructions().await?;

    Ok(chain.estimate_fee_v1(&instructions, &params).await?)
}

pub(super) async fn withdraw(
    chain: &SolanaChain,
    req: &WithdrawReq,
    amount: alloy::primitives::U256,
    fee: String,
    key: ChainPrivateKey,
) -> Result<TransferResp, crate::ServiceError> {
    let fee_setting = serde_func::serde_from_str::<SolFeeSetting>(&fee)?;

    let params = Withdraw::new_wsol(&req.from, amount, &chain.get_provider())?;

    let instructions = params.instructions().await?;

    // check balance
    let balance = chain.balance(&req.from, None).await?;
    if U256::from(fee_setting.original_fee()) > balance {
        return Err(crate::BusinessError::Chain(
            crate::ChainError::InsufficientFeeBalance,
        ))?;
    }

    // 验证手续费,进行拦截
    let tx_hash = chain
        .exec_transaction(params, key, None, instructions, 0)
        .await?;

    let fee = fee_setting.transaction_fee().to_string();
    Ok(TransferResp {
        tx_hash,
        fee,
        consumer: None,
    })
}
