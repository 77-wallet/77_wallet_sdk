#![feature(trait_upcasting, let_chains)]
pub mod api;
pub mod directory_structure;
pub mod error;
pub mod file_ops;
pub mod naming;
pub mod service;
pub mod utils;
pub mod wallet_hierarchy;

pub use error::Error;

pub use crate::wallet_hierarchy::WalletTreeStrategy;
pub use naming::v2::KeyMeta;
pub use service::Keystore;
pub use wallet_keystore::KdfAlgorithm;
