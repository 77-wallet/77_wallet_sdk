#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    #[error("Wallet already exists")]
    AlreadyExist,
    #[error("Wallet not exist")]
    NotFound,
    #[error("Password incorrect")]
    PasswordIncorrect,
    #[error("Password not set")]
    PasswordNotSet,
    #[error("Wallet not init")]
    NotInit,
    #[error("Wallet type not set")]
    WalletTypeNotSet,
}

impl WalletError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            WalletError::AlreadyExist => 3100,
            WalletError::NotFound => 3101,
            WalletError::PasswordIncorrect => 3102,
            WalletError::PasswordNotSet => 3103,
            WalletError::NotInit => 3104,
            WalletError::WalletTypeNotSet => 3105,
        }
    }
}
