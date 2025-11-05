#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    #[error("This mnemonic phrase has been imported into the normal wallet system")]
    MnemonicAlreadyImportedIntoNormalWalletSystem,
    #[error("Api Wallet not exist, please check your input")]
    NotFound,
    // APPID已绑定，请勿重复操作
    #[error("AppId already binded, do not repeat operation")]
    AppIdAlreadyBinded,
    // 该出款钱包未在该appId下使用过
    #[error("This withdrawal wallet has not been used under this appId")]
    WithdrawalWalletNotUsed,
    // 子账户钱包未绑定
    #[error("The sub account wallet is unbound")]
    SubAccountWalletNotBound,
    #[error("Api Wallet already exists")]
    AlreadyExist,
}

impl WalletError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            WalletError::MnemonicAlreadyImportedIntoNormalWalletSystem => 20000,
            WalletError::NotFound => 20001,
            WalletError::AppIdAlreadyBinded => 20002,
            WalletError::WithdrawalWalletNotUsed => 20003,
            WalletError::SubAccountWalletNotBound => 20004,
            WalletError::AlreadyExist => 20005,
        }
    }
}
