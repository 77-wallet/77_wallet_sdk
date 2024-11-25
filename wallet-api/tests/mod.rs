use std::{env, path::PathBuf};
use wallet_api::WalletManager;
use wallet_utils::init_test_log;

mod config;
mod multisig_tx;
mod phrase;
mod stake;

async fn get_manager() -> WalletManager {
    init_test_log();
    let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("test_data")
        .to_string_lossy()
        .to_string();
    WalletManager::new("sn", "ANDROID", &path, None)
        .await
        .unwrap()
}
