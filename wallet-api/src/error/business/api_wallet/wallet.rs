#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    // APPID已绑定，请勿重复操作
    #[error("AppId already binded, do not repeat operation")]
    AppIdAlreadyBinded,
    // 该出款钱包未在该appId下使用过
    #[error("This withdrawal wallet has not been used under this appId")]
    WithdrawalWalletNotUsed,
    #[error("Wallet not exist, please check your input")]
    NotFound,
}

impl WalletError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            WalletError::AppIdAlreadyBinded => 3100,
            WalletError::WithdrawalWalletNotUsed => 3101,
            WalletError::NotFound => 3102,
        }
    }
}
