#[derive(Debug, thiserror::Error, serde::Serialize)]
pub enum Error {
    // #[error("Net failed: {0:?}")]
    // Net(#[from] super::common::net::NetError),
    // #[error("Service error: {0}")]
    // Service(String),
    #[error("Unknown language")]
    UnknownLanguage,
    #[error("Unknown query mode")]
    UnknownQueryMode,
    #[error("Unknown chain code")]
    UnknownChainCode,
    #[error("Unknown coin type: {0}")]
    UnknownCoinType(u32),
    #[error("Mnemonic: {0}")]
    Mnemonic(String),
}

impl From<coins_bip39::MnemonicError> for Error {
    fn from(value: coins_bip39::MnemonicError) -> Self {
        Error::Mnemonic(value.to_string())
    }
}
