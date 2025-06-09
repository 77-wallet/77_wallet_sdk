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

// #[derive(Debug, Clone, serde::Deserialize)]
// pub(crate) struct Coins {
//     pub(crate) coins: Vec<DefaultCoin>,
// }

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct DefaultCoinList {
    pub(crate) coins: Vec<DefaultCoin>,
}

static INIT_COINS_INFO: once_cell::sync::Lazy<once_cell::sync::OnceCell<DefaultCoinList>> =
    once_cell::sync::Lazy::new(once_cell::sync::OnceCell::new);

pub(crate) fn init_default_coins_list() -> Result<&'static DefaultCoinList, crate::ServiceError> {
    INIT_COINS_INFO.get_or_try_init(|| {
        // let mut res = std::collections::HashMap::new();
        let toml_content = include_str!("../../data/config/coin.toml");
        let toml_data: DefaultCoinList = wallet_utils::serde_func::toml_from_str(toml_content)?;
        Ok(toml_data)
    })
}

impl From<DefaultCoin> for wallet_transport_backend::CoinInfo {
    fn from(value: DefaultCoin) -> Self {
        Self {
            symbol: Some(value.symbol),
            name: Some(value.name),
            chain_code: Some(value.chain_code),
            token_address: value.token_address,
            protocol: value.protocol,
            id: Default::default(),
            decimals: Some(value.decimals),
            default_token: value.default,
            popular_token: value.popular,
            enable: value.active,
        }
    }
}
