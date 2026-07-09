mod broadcast_flow;
pub(crate) mod broadcast_mechanism;
pub(crate) mod collect;
pub(crate) mod fee;
mod nonce_flow;
mod recover_flow;
mod recover_mechanism;
pub(crate) mod shared_mechanism;
mod transfer_flow;
pub(crate) mod withdraw;

#[cfg(test)]
mod confirm_tx_tests;

pub(crate) struct ApiTransDomain {}
