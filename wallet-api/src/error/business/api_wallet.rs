#[derive(Debug, thiserror::Error)]
pub enum ApiWalletError {
    #[error("This mnemonic phrase has been imported into the normal wallet system")]
    MnemonicAlreadyImportedIntoNormalWalletSystem,
    #[error("Api Wallet already exists")]
    AlreadyExist,
    #[error("Api Wallet not exist")]
    NotFound,
    #[error("Chain config not found: `{0}`")]
    ChainConfigNotFound(String),
}

impl ApiWalletError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            ApiWalletError::MnemonicAlreadyImportedIntoNormalWalletSystem => 4400,
            ApiWalletError::AlreadyExist => 4401,
            ApiWalletError::NotFound => 4402,
            ApiWalletError::ChainConfigNotFound(_) => 4403,
        }
    }
}
