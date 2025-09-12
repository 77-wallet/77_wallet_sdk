#![allow(unused)]
use serde::Deserialize;
use serde_yaml;
use std::{fs, path::Path};
use wallet_oss::OssConfig;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub oss: OssConfig,
    pub backend_api: BackendApiConfig,
    pub aggregate_api: AggregateApi,
    pub crypto: CryptoConfig,
    pub app_code: String,
}

#[derive(Deserialize, Debug)]
pub struct CryptoConfig {
    pub aes_key: String,
    pub aes_iv: String,
}

#[derive(Deserialize, Debug)]
pub struct AggregateApi {
    pub dev_url: String,
    pub test_url: String,
    pub prod_url: String,
}

#[derive(Deserialize, Debug)]
pub struct BackendApiConfig {
    pub dev_url: String,
    pub test_url: String,
    pub prod_url: String,
}

impl Config {
    pub fn new(config_content: &str) -> Result<Self, crate::ServiceError> {
        let config: Config = wallet_utils::serde_func::serde_yaml_from_str(config_content)?;
        Ok(config)
    }
}
