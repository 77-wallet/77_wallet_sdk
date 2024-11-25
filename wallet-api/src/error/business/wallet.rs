#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    // #[error("Community error: {0}")]
    // Community(#[from] super::api::community::CommunityError),
    // #[error("Account error: {0}")]
    // Account(#[from] super::api::account::AccountError),
    #[error("Wallet already exists")]
    AlreadyExist,
    #[error("Wallet not exist")]
    NotFound,
}

impl WalletError {
    pub(crate) fn get_status_code(&self) -> u32 {
        match self {
            WalletError::AlreadyExist => 3100,
            WalletError::NotFound => 3101,
        }
    }
}
