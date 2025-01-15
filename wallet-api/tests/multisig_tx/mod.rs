pub mod multisig_bnb;
pub mod multisig_btc;
pub mod multisig_sol;
pub mod multisig_tron;

use std::{env, path::PathBuf};
use wallet_api::WalletManager;
use wallet_utils::init_test_log;

async fn get_manager() -> WalletManager {
    init_test_log();
    let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("test_data")
        .to_string_lossy()
        .to_string();
    let config = wallet_api::Config::new(&wallet_api::test::env::get_config().unwrap()).unwrap();
    WalletManager::new("sn", "ANDROID", &path, None, config)
        .await
        .unwrap()
}
