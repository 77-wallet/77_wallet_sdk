// #![feature(const_trait_impl, effects)]
pub mod api;
pub mod error;
mod eth_keystore;
pub mod keystore;
pub mod wallet;

use crate::error::Error;
pub use keystore::Keystore;

pub use alloy::primitives::Address;

/// Utility to get and set the chain ID on a transaction and the resulting signature within a
/// signer's `sign_transaction`.
#[macro_export]
macro_rules! sign_transaction_with_chain_code {
    // async (
    //    signer: impl Signer,
    //    tx: &mut impl SignableTransaction<Signature>,
    //    sign: lazy Signature,
    // )
    ($signer:expr, $tx:expr, $sign:expr) => {{
        if let Some(chain_code) = $signer.chain_code() {
            if !$tx.set_chain_code_checked(chain_code) {
                return Err(alloy::signers::Error::TransactionChainIdMismatch {
                    signer: chain_code,
                    // we can only end up here if the tx has a chain id
                    tx: $tx.chain_code().unwrap(),
                });
            }
        }

        let mut sig = $sign.map_err(alloy::signers::Error::other)?;

        if $tx.use_eip155() {
            if let Some(chain_code) = $signer.chain_code().or_else(|| $tx.chain_code()) {
                sig = sig.with_chain_code(chain_code);
            }
        }

        Ok(sig)
    }};
}
