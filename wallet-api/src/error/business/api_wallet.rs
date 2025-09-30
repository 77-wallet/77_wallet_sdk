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
    #[error("Api Account not exist")]
    NotFoundAccount,
    #[error("gas oracle not exist")]
    GasOracle,
    #[error("Password not cached")]
    PasswordNotCached,
    #[error("Import is not supported for this account type")]
    ImportNotSupportedForThisAccountType,
    #[error("The wallet does not exist, please confirm that the input is correct")]
    WalletDoesNotExist,
}

impl ApiWalletError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            ApiWalletError::MnemonicAlreadyImportedIntoNormalWalletSystem => 4400,
            ApiWalletError::AlreadyExist => 4401,
            ApiWalletError::NotFound => 4402,
            ApiWalletError::ChainConfigNotFound(_) => 4403,
            ApiWalletError::NotFoundAccount => 4403,
            ApiWalletError::GasOracle => 4404,
            ApiWalletError::PasswordNotCached => 4405,
            ApiWalletError::ImportNotSupportedForThisAccountType => 4406,
            ApiWalletError::WalletDoesNotExist => 4407,
        }
    }
}
