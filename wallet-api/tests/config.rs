use std::{env, path::PathBuf};
use wallet_api::WalletManager;
use wallet_database::entities::config::{config_key::MIN_VALUE_SWITCH, MinValueSwitchConfig};
use wallet_utils::init_test_log;

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

#[tokio::test]
async fn test_config_list() {
    let wallet_manager = get_manager().await;

    let configs = wallet_manager.get_configs().await;

    tracing::info!("{}", serde_json::to_string(&configs).unwrap());
}

#[tokio::test]
async fn test_set_min_value_config() {
    let wallet_manager = get_manager().await;

    let config = MinValueSwitchConfig {
        switch: false,
        value: 2.0,
        currency: "USD".to_string(),
    };

    let key = MIN_VALUE_SWITCH.to_string();

    let configs = wallet_manager
        .set_config(key, config.to_json_str().unwrap())
        .await;
    tracing::info!("{:?}", serde_json::to_string(&configs).unwrap());
}
