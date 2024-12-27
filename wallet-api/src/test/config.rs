use std::fs;

use serde::Deserialize;

use crate::{CreateAccountReq, CreateWalletReq, InitDeviceReq};

#[derive(Deserialize, Debug)]
pub struct Config {
    // pub database_path: String,
    // pub example_env: ExampleEnv,
    // pub key_length: usize,
}

pub fn load_config(config_path: &str) -> Result<Config, anyhow::Error> {
    let config_str = fs::read_to_string(config_path)?;
    let config: Config = serde_json::from_str(&config_str)?;
    Ok(config)
}

// pub fn initialize(config_path: &str) -> Result<(), anyhow::Error> {
//     let config = crate::config::load_config(config_path)?;

//     // db::init_db(&config.database_path)?;
//     // key::generate_key_file(&config.key_file_path, config.key_length)?;

//     Ok(())
// }

#[derive(Deserialize, Debug)]
pub struct TestParams {
    pub device_req: InitDeviceReq,
    pub create_wallet_req: CreateWalletReq,
    pub create_account_req: CreateAccountReq,
}
