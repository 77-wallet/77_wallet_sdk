use alloy::primitives::U256;

use crate::{domain::api_wallet::adapter::sol_tx::SolTx, error::service::ServiceError};

/// Test-facing wrapper for the Solana native SOL rent precheck.
///
/// Integration tests use this helper to exercise the shared withdraw / transfer
/// rent rule without starting the full wallet manager stack.
pub fn sol_native_transfer_rent_precheck(
    from: &str,
    to: &str,
    recipient_exists: bool,
    payer_balance: U256,
    transfer_amount: U256,
    minimum_rent: U256,
) -> Result<(), ServiceError> {
    SolTx::native_transfer_rent_precheck(
        from,
        to,
        recipient_exists,
        payer_balance,
        transfer_amount,
        minimum_rent,
    )
}
