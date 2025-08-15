use crate::domain::chain::swap::evm_swap::SwapParams;
use wallet_chain_interact::sol::SolanaChain;

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
