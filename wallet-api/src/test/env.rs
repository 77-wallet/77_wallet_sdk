use anyhow::Result;
use std::{env, path::PathBuf};
use tracing::info;

use crate::WalletManager;

pub async fn get_manager() -> Result<(WalletManager, super::config::TestParams)> {
    // 获取项目根目录
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);

    let config_dir = dir.join("example").join("config.toml");
    println!("example_dir: {config_dir:?}");
    let config_data = std::fs::read_to_string(config_dir)?;

    let test_params: crate::test::config::TestParams =
        wallet_utils::serde_func::toml_from_str(&config_data)?;
    // std::env::set_var("RUST_BACKTRACE", "1");

    let client_id = "test_data";
    // 获取项目根目录
    let storage_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join(&client_id);

    // 创建测试目录
    if !storage_dir.exists() {
        std::fs::create_dir_all(&storage_dir)?;
    }

    // 测试参数

    // if temp {
    //     info!("storage_dir: {:?}", storage_dir);
    //     // 创建临时目录结构
    //     let temm_dir = tempfile::tempdir_in(&storage_dir)?;
    //     wallet_name = temm_dir
    //         .path()
    //         .file_name()
    //         .map(|name| name.to_string_lossy().to_string());
    // }

    info!("[setup_test_environment] storage_dir: {:?}", storage_dir);
    let wallet_manager = WalletManager::new(
        &test_params.device_req.sn,
        &test_params.device_req.device_type,
        &storage_dir.to_string_lossy(),
        None,
        "https://test-api.puke668.top",
    )
    .await?;
    // let derivation_path = "m/44'/60'/0'/0/1".to_string();

    Ok((wallet_manager, test_params))
}
