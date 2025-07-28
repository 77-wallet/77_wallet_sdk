#[derive(Debug, thiserror::Error)]
pub enum ApiWalletError {
    #[error("This mnemonic phrase has been imported into the normal wallet system")]
    MnemonicAlreadyImportedIntoNormalWalletSystem,
}

impl ApiWalletError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            ApiWalletError::MnemonicAlreadyImportedIntoNormalWalletSystem => 4400,
        }
    }
}
