#[derive(Debug, thiserror::Error)]
pub enum ChainError {
    #[error("Chain not found: {0}")]
    NotFound(String),
    #[error("Insufficient balance")]
    InsufficientBalance,
    #[error("Insufficient balance for fees")]
    InsufficientFeeBalance,
    #[error("btc address type cannot be empty")]
    BitcoinAddressEmpty,
    #[error("address format incorrect")]
    AddressFormatIncorrect,
    #[error("address is Frozen")]
    AddressIsFrozen,
    #[error("amount less than min amount")]
    AmountLessThanMin,
    #[error("address not init on chain")]
    AddressNotInit,
    #[error("The chain does not support this operation")]
    NotSupportChain,
    #[error("get node token err pelase change node!")]
    NodeToken(String),
    // 不满足最小租金from地址或者to地址。
    #[error("sol transfer balance less rent")]
    InsufficientFundsRent,
    #[error("btc exceeds max fee")]
    ExceedsMaximum,
    #[error("Dust transaction")]
    DustTransaction,
    #[error("Exceeds Max Fee")]
    ExceedsMaxFeerate,
    // 波场没有奖励提取
    #[error("no reward claim")]
    NoRewardClaim,
}

impl ChainError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            ChainError::NotFound(_) => 3501,
            ChainError::InsufficientBalance => 3502,
            ChainError::InsufficientFeeBalance => 3503,
            ChainError::BitcoinAddressEmpty => 3504,
            ChainError::AddressFormatIncorrect => 3505,
            ChainError::AddressIsFrozen => 3506,
            ChainError::AmountLessThanMin => 3507,
            ChainError::AddressNotInit => 3508,
            ChainError::NotSupportChain => 3509,
            ChainError::NodeToken(_) => 3510,
            ChainError::InsufficientFundsRent => 3511,
            ChainError::ExceedsMaximum => 3512,
            ChainError::DustTransaction => 3513,
            ChainError::ExceedsMaxFeerate => 3514,
            ChainError::NoRewardClaim => 3515,
        }
    }
}
