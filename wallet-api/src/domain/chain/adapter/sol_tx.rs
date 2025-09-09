use crate::{domain::chain::swap::evm_swap::SwapParams, request::transaction::DepositReq};
use wallet_chain_interact::sol::{self, SolanaChain};

pub(super) async fn estimate_swap(
    swap_params: &SwapParams,
    chain: &SolanaChain,
) -> Result<(), crate::ServiceError> {
    Ok(())
}

pub(super) async fn swap(
    swap_params: &SwapParams,
    chain: &SolanaChain,
) -> Result<(), crate::ServiceError> {
    Ok(())
}

pub(super) async fn deposit_fee(
    chain: &SolanaChain,
    req: &DepositReq,
    amount: alloy::primitives::U256,
) -> Result<(), crate::ServiceError> {
    // let deposit = sol::operations::contract::Deposit::new(&req.from, &req.token, amount);

    Ok(())
}
