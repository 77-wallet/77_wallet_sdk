use crate::{dirs::Dirs, manager::WalletManager};
use anyhow::Result;
use std::{env, path::PathBuf};
use tracing::info;

use crate::{request::account::CreateAccountReq, request::wallet::CreateWalletReq, request::devices::InitDeviceReq};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct TestParams {
    pub device_req: InitDeviceReq,
    pub create_wallet_req: CreateWalletReq,
    pub create_account_req: CreateAccountReq,
}

pub async fn get_manager() -> Result<(WalletManager, TestParams)> {
    // 获取项目根目录
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);

    let config_dir = dir.join("examples/").join("config.toml");
    println!("example_dir: {config_dir:?}");
    let config_data = std::fs::read_to_string(config_dir)?;

    let test_params: TestParams = wallet_utils::serde_func::toml_from_str(&config_data)?;
    // std::env::set_var("RUST_BACKTRACE", "1");

    let client_id = "test_data";
    // 获取项目根目录
    let storage_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join(client_id);

    // 创建测试目录
    if !storage_dir.exists() {
        std::fs::create_dir_all(&storage_dir)?;
    }

    info!("[setup_test_environment] storage_dir: {:?}", storage_dir);

    let dirs = Dirs::new(&storage_dir.to_string_lossy())?;
    let config = crate::config::Config::new(&crate::test::env::get_config()?)?;
    let wallet_manager = WalletManager::new(
        &test_params.device_req.sn,
        &test_params.device_req.device_type,
        None,
        config,
        dirs,
    )
    .await?;
    // let derivation_path = "m/44'/60'/0'/0/1".to_string();

    Ok((wallet_manager, test_params))
}

pub fn get_config() -> Result<String> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| std::env::current_dir().unwrap().to_string_lossy().into_owned());
    let dir = PathBuf::from(manifest_dir);
    let config_dir = dir.join("examples").join("config.yaml");
    let config_data = std::fs::read_to_string(config_dir)?;
    Ok(config_data)
}
