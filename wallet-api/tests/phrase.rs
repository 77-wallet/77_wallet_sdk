use std::{env, path::PathBuf};

use wallet_api::WalletManager;
use wallet_utils::init_test_log;

async fn get_manager() -> WalletManager {
    init_test_log();
    let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("test_data")
        .to_string_lossy()
        .to_string();
    WalletManager::new("sn", "ANDROID", &path, None, "https://test-api.puke668.top")
        .await
        .unwrap()
}

#[tokio::test]
async fn test_phrase() {
    let wallet_manager = get_manager().await;

    let phrase = wallet_manager.generate_phrase(1, 12);

    //
    tracing::info!("{phrase:?}")
}
