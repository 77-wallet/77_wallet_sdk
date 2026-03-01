use crate::{config::Config, error::service::ServiceError};

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct DefaultCoin {
    pub(crate) name: String,
    pub(crate) chain_code: String,
    pub(crate) symbol: String,
    pub(crate) decimals: u8,
    pub(crate) token_address: Option<String>,
    pub(crate) protocol: Option<String>,
    pub(crate) default: bool,
    pub(crate) popular: bool,
    pub(crate) active: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct DefaultCoinList {
    pub(crate) coins: Vec<DefaultCoin>,
}

static INIT_COINS_INFO: once_cell::sync::Lazy<once_cell::sync::OnceCell<DefaultCoinList>> =
    once_cell::sync::Lazy::new(once_cell::sync::OnceCell::new);

pub(crate) fn init_default_coins_list() -> Result<&'static DefaultCoinList, ServiceError> {
    let network = Config::feature_chain_network();
    let (profile, cell, toml_content) = match network {
        crate::config::ChainNetwork::Mainnet => {
            ("mainnet", &*INIT_COINS_INFO, include_str!("../../data/config/coin.mainnet.toml"))
        }
        crate::config::ChainNetwork::Testnet => {
            ("testnet", &*INIT_COINS_INFO, include_str!("../../data/config/coin.testnet.toml"))
        }
    };
    tracing::info!("loading default coin config profile={}", profile);
    cell.get_or_try_init(|| {
        let toml_data: DefaultCoinList = wallet_utils::serde_func::toml_from_str(toml_content)?;
        Ok(toml_data)
    })
}
