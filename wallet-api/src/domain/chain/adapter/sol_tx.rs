use crate::{
    domain::chain::{TransferResp, swap::EstimateSwapResult},
    infrastructure::swap_client::SolInstructResp,
    request::transaction::{DepositReq, SwapReq, WithdrawReq},
};
use alloy::primitives::U256;
use wallet_chain_interact::{
    sol::{
        SolFeeSetting, SolanaChain,
        consts::SOL_DECIMAL,
        operations::{
            SolInstructionOperation,
            native_coin::{Deposit, Swap, Withdraw},
        },
    },
    types::ChainPrivateKey,
};
use wallet_utils::serde_func;

// 默认系统账号的租金
pub const SYSTEM_ACCOUNT_RENT: f64 = 0.000990880;

// 默认token账号的租金
pub const TOKEN_ACCOUNT_REND: u64 = 2500000;

// 默认的手续费(用于展示)
pub const DEFAULT_UNITS_FEE: u64 = 5000;

pub(super) async fn estimate_swap(
    payer: &str,
    instructions: SolInstructResp,
    chain: &SolanaChain,
) -> Result<EstimateSwapResult<u64>, crate::ServiceError> {
    let payer = wallet_utils::address::parse_sol_address(payer)?;

    let res = chain
        .similar_transaction(&payer, &instructions.ins, Some(instructions.alts.clone()))
        .await?;
    if res.value.err.is_some() {
        // tracing::error!("swap simulate error: {:?}", res);
        // let _res =
        //     FrontendNotifyEvent::send_debug(&wallet_utils::serde_func::serde_to_value(&res.value)?)
        //         .await;
        let log = res.value.logs.join(",");

        // 如果有超时的错误
        let error = if log.contains("RequireGteViolated") {
            crate::BusinessError::Chain(crate::ChainError::SolSwapTime(log))
        } else {
            crate::BusinessError::Chain(crate::ChainError::SwapSimulate(log))
        };

        return Err(error)?;
    }

    //
    let return_data = res.value.return_data;

    let mut amount_in = 0_u64;
    let mut amount_out = 0_u64;
    if let Some(return_data) = return_data {
        let bytes = wallet_utils::base64_to_bytes(&return_data.data[0])?;
        amount_in = u64::from_le_bytes(bytes[0..8].try_into().map_err(|_| {
            crate::ServiceError::Parameter("sol simultate parse return data error in".to_string())
        })?);

        amount_out = u64::from_le_bytes(bytes[8..16].try_into().map_err(|_| {
            crate::ServiceError::Parameter("sol simultate parse return data error out".to_string())
        })?);
    }

    let consumer = res.value.units_consumed;

    let res = EstimateSwapResult {
        amount_in: U256::from(amount_in),
        amount_out: U256::from(amount_out),
        consumer,
    };

    Ok(res)
}

pub(super) async fn swap(
    chain: &SolanaChain,
    req: &SwapReq,
    fee: String,
    instructions: SolInstructResp,
    key: ChainPrivateKey,
) -> Result<TransferResp, crate::ServiceError> {
    let fee_setting = wallet_utils::serde_func::serde_from_str::<SolFeeSetting>(&fee)?;
    let fee = fee_setting.transaction_fee().to_string();

    // check balance (sol 减租金)
    let mut balance = chain.balance(&req.recipient, None).await?;
    balance -= wallet_utils::unit::convert_to_u256(&SYSTEM_ACCOUNT_RENT.to_string(), SOL_DECIMAL)?;

    // 如果是主币兑换那么 需要考虑余额的情况
    let mut origin_fee = U256::from(fee_setting.original_fee());
    if req.token_in.token_addr.is_empty() {
        let amount_in =
            wallet_utils::unit::convert_to_u256(&req.amount_in, req.token_in.decimals as u8)?;
        origin_fee += amount_in;
    }
    if origin_fee > balance {
        return Err(crate::BusinessError::Chain(
            crate::ChainError::InsufficientFeeBalance,
        ))?;
    }

    let params = Swap::new(&req.recipient)?;

    let tx_hash = chain
        .exec_v0_transaction(
            params,
            key,
            Some(fee_setting),
            instructions.ins,
            instructions.alts,
        )
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
    let params = Deposit::new_wsol(&req.from, amount, chain.get_provider())?;

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
    let params = Deposit::new_wsol(&req.from, amount, chain.get_provider())?;

    let instructions = params.instructions().await?;

    // check balance
    let mut balance = chain.balance(&req.from, None).await?;
    balance -= U256::from(SYSTEM_ACCOUNT_RENT);
    if U256::from(fee_setting.original_fee()) + amount > balance {
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
    let params = Withdraw::new_wsol(&req.from, amount, chain.get_provider())?;

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
    let fee = fee_setting.transaction_fee().to_string();

    let params = Withdraw::new_wsol(&req.from, amount, chain.get_provider())?;
    let instructions = params.instructions().await?;

    // check balance
    let mut balance = chain.balance(&req.from, None).await?;
    balance -= U256::from(SYSTEM_ACCOUNT_RENT);
    if U256::from(fee_setting.original_fee()) > balance {
        return Err(crate::BusinessError::Chain(
            crate::ChainError::InsufficientFeeBalance,
        ))?;
    }

    // 验证手续费,进行拦截
    let tx_hash = chain
        .exec_transaction(params, key, None, instructions, 0)
        .await?;

    Ok(TransferResp {
        tx_hash,
        fee,
        consumer: None,
    })
}
