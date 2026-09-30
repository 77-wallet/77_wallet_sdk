use crate::error::service::ServiceError;
use wallet_database::entities::asset_token_key::AssetTokenKey;

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct DefaultCoin {
    pub(crate) name: String,
    pub(crate) chain_code: String,
    pub(crate) symbol: String,
    pub(crate) decimals: u8,
    #[serde(default)]
    pub(crate) token_address: AssetTokenKey,
    pub(crate) protocol: Option<String>,
    pub(crate) default: bool,
    pub(crate) popular: bool,
    pub(crate) active: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct DefaultCoinList {
    pub(crate) coins: Vec<DefaultCoin>,
}

static INIT_MAINNET_COINS_INFO: once_cell::sync::Lazy<once_cell::sync::OnceCell<DefaultCoinList>> =
    once_cell::sync::Lazy::new(once_cell::sync::OnceCell::new);
static INIT_TESTNET_COINS_INFO: once_cell::sync::Lazy<once_cell::sync::OnceCell<DefaultCoinList>> =
    once_cell::sync::Lazy::new(once_cell::sync::OnceCell::new);

fn init_default_coins_list_by_profile(
    profile: &'static str,
) -> Result<&'static DefaultCoinList, ServiceError> {
    let (cell, toml_content) = match profile {
        "mainnet" => {
            (&*INIT_MAINNET_COINS_INFO, include_str!("../../data/config/coin.mainnet.toml"))
        }
        "testnet" => {
            (&*INIT_TESTNET_COINS_INFO, include_str!("../../data/config/coin.testnet.toml"))
        }
        _ => {
            return Err(crate::error::business::BusinessError::Coin(
                crate::error::business::coin::CoinError::NotFound(profile.to_string()),
            )
            .into());
        }
    };

    tracing::info!("loading default coin config profile={}", profile);
    cell.get_or_try_init(|| {
        let toml_data: DefaultCoinList = wallet_utils::serde_func::toml_from_str(toml_content)?;
        Ok(toml_data)
    })
}

pub(crate) fn mainnet_default_coins_list() -> Result<&'static DefaultCoinList, ServiceError> {
    init_default_coins_list_by_profile("mainnet")
}

pub(crate) fn testnet_default_coins_list() -> Result<&'static DefaultCoinList, ServiceError> {
    init_default_coins_list_by_profile("testnet")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn added_evm_chains_have_native_testnet_coins() {
        let coins = &testnet_default_coins_list().expect("load testnet coins").coins;
        let expected = [
            ("polygon", "POL"),
            ("avalanche", "AVAX"),
            ("arbitrum", "ETH"),
            ("base", "ETH"),
            ("scroll", "ETH"),
        ];

        for (chain_code, symbol) in expected {
            let coin = coins
                .iter()
                .find(|coin| coin.chain_code == chain_code && coin.symbol == symbol)
                .unwrap_or_else(|| panic!("missing native coin for {chain_code}"));
            assert_eq!(coin.decimals, 18);
            assert_eq!(coin.protocol.as_deref(), Some("ERC20"));
            assert!(coin.token_address.as_db_str().is_empty());
            assert!(coin.default && coin.popular && coin.active);
        }
        assert!(!coins.iter().any(|coin| coin.chain_code == "fantom"));
    }
}
