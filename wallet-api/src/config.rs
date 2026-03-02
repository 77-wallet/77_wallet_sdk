use serde::Deserialize;
use wallet_oss::OssConfig;

// 运行时稳定性相关的默认阈值（本轮止血参数）集中放在子模块中维护。
pub mod runtime_defaults;

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChainNetwork {
    Mainnet,
    Testnet,
}

impl ChainNetwork {
    pub fn as_str(self) -> &'static str {
        match self {
            ChainNetwork::Mainnet => "mainnet",
            ChainNetwork::Testnet => "testnet",
        }
    }

    pub fn to_network_kind(self) -> wallet_types::chain::network::NetworkKind {
        match self {
            ChainNetwork::Mainnet => wallet_types::chain::network::NetworkKind::Mainnet,
            ChainNetwork::Testnet => wallet_types::chain::network::NetworkKind::Testnet,
        }
    }
}

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
    pub fn new(config_content: &str) -> Result<Self, crate::error::service::ServiceError> {
        let config: Config = wallet_utils::serde_func::serde_yaml_from_str(config_content)?;
        Ok(config)
    }

    pub fn resolved_chain_network(&self) -> ChainNetwork {
        Self::feature_chain_network()
    }

    /// NOTE:
    /// feature network is a compile-profile compatibility value only.
    /// Runtime chain network must be resolved from the bound node.network per chain.
    pub fn feature_chain_network() -> ChainNetwork {
        #[cfg(feature = "prod")]
        {
            return ChainNetwork::Mainnet;
        }
        #[cfg(any(feature = "test", feature = "dev"))]
        {
            return ChainNetwork::Testnet;
        }
        ChainNetwork::Mainnet
    }

    pub fn active_feature_profile() -> &'static str {
        #[cfg(feature = "prod")]
        {
            return "prod";
        }
        #[cfg(feature = "dev")]
        {
            return "dev";
        }
        #[cfg(feature = "test")]
        {
            return "test";
        }
        "unknown"
    }
}
