use std::collections::HashMap;
use crate::error::ServiceError;

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

static INIT_CHAINS_INFO: once_cell::sync::Lazy<once_cell::sync::OnceCell<DefaultChainList>> =
    once_cell::sync::Lazy::new(once_cell::sync::OnceCell::new);

pub(crate) fn get_default_chains_list() -> Result<&'static DefaultChainList, ServiceError> {
    INIT_CHAINS_INFO.get_or_try_init(|| {
        let toml_content = include_str!("../../data/config/chain.toml");
        let toml_data: DefaultChainList = wallet_utils::serde_func::toml_from_str(toml_content)?;

        Ok(toml_data)
    })
}
