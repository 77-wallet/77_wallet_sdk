use crate::error::service::ServiceError;
use std::collections::HashMap;

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct DefaultChain {
    pub(crate) name: String,
    pub(crate) chain_code: String,
    pub(crate) protocols: Vec<String>,
    pub(crate) main_symbol: String,
    pub(crate) active: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct DefaultChainList {
    pub(crate) chains: HashMap<String, DefaultChain>,
}

static INIT_MAINNET_CHAINS_INFO: once_cell::sync::Lazy<
    once_cell::sync::OnceCell<DefaultChainList>,
> = once_cell::sync::Lazy::new(once_cell::sync::OnceCell::new);
static INIT_TESTNET_CHAINS_INFO: once_cell::sync::Lazy<
    once_cell::sync::OnceCell<DefaultChainList>,
> = once_cell::sync::Lazy::new(once_cell::sync::OnceCell::new);

pub(crate) fn mainnet_default_chains_list() -> Result<&'static DefaultChainList, ServiceError> {
    INIT_MAINNET_CHAINS_INFO.get_or_try_init(|| {
        let toml_content = include_str!("../../data/config/chain.mainnet.toml");
        let toml_data: DefaultChainList = wallet_utils::serde_func::toml_from_str(toml_content)?;

        Ok(toml_data)
    })
}

pub(crate) fn testnet_default_chains_list() -> Result<&'static DefaultChainList, ServiceError> {
    INIT_TESTNET_CHAINS_INFO.get_or_try_init(|| {
        let toml_content = include_str!("../../data/config/chain.testnet.toml");
        let toml_data: DefaultChainList = wallet_utils::serde_func::toml_from_str(toml_content)?;

        Ok(toml_data)
    })
}

pub(crate) fn default_chain_lists_for_feature(
    profile: &'static str,
) -> Result<Vec<&'static DefaultChainList>, ServiceError> {
    match profile {
        "prod" => Ok(vec![mainnet_default_chains_list()?]),
        "dev" | "test" => Ok(vec![mainnet_default_chains_list()?, testnet_default_chains_list()?]),
        other => Err(ServiceError::Parameter(format!(
            "unsupported default chain config profile: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn added_evm_chains_are_testnet_only() {
        let mainnet = &mainnet_default_chains_list().expect("load mainnet chains").chains;
        let testnet = &testnet_default_chains_list().expect("load testnet chains").chains;
        let expected = [
            ("polygon", "POL"),
            ("avalanche", "AVAX"),
            ("fantom", "FTM"),
            ("arbitrum", "ETH"),
            ("base", "ETH"),
            ("scroll", "ETH"),
        ];

        for (chain_code, main_symbol) in expected {
            assert!(!mainnet.contains_key(chain_code), "{chain_code} leaked into mainnet");
            let chain = testnet.get(chain_code).unwrap_or_else(|| panic!("missing {chain_code}"));
            assert_eq!(chain.chain_code, chain_code);
            assert_eq!(chain.main_symbol, main_symbol);
            assert_eq!(chain.protocols, ["ERC20"]);
            assert!(chain.active);
        }
    }

    #[test]
    fn chain_profiles_select_expected_config_sets() {
        let prod = default_chain_lists_for_feature("prod").expect("prod chain configs");
        let dev = default_chain_lists_for_feature("dev").expect("dev chain configs");
        let test = default_chain_lists_for_feature("test").expect("test chain configs");

        assert_eq!(prod.len(), 1);
        assert_eq!(dev.len(), 2);
        assert_eq!(test.len(), 2);
        assert!(prod[0].chains.contains_key("eth"));
        assert!(dev.iter().any(|list| list.chains.contains_key("polygon")));
        assert!(test.iter().any(|list| list.chains.contains_key("scroll")));
    }
}
