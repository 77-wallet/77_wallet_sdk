use std::fmt;

#[derive(Debug, Clone, Default)]
pub struct InsufficientBalanceDetail {
    pub from_addr: Option<String>,
    pub to_addr: Option<String>,
    pub chain_code: Option<String>,
    pub token_addr: Option<String>,
    pub value: Option<String>,
    pub balance: Option<String>,
    pub need: Option<String>,
    pub fee: Option<String>,
    pub reason: Option<String>,
}

impl fmt::Display for InsufficientBalanceDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(v) = &self.reason {
            parts.push(format!("reason={v}"));
        }
        if let Some(v) = &self.from_addr {
            parts.push(format!("from_addr={v}"));
        }
        if let Some(v) = &self.to_addr {
            parts.push(format!("to_addr={v}"));
        }
        if let Some(v) = &self.chain_code {
            parts.push(format!("chain_code={v}"));
        }
        if let Some(v) = &self.token_addr {
            parts.push(format!("token_addr={v}"));
        }
        if let Some(v) = &self.value {
            parts.push(format!("value={v}"));
        }
        if let Some(v) = &self.balance {
            parts.push(format!("balance={v}"));
        }
        if let Some(v) = &self.need {
            parts.push(format!("need={v}"));
        }
        if let Some(v) = &self.fee {
            parts.push(format!("fee={v}"));
        }

        if parts.is_empty() {
            return Ok(());
        }

        write!(f, ": {}", parts.join(", "))
    }
}

impl InsufficientBalanceDetail {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_reason(reason: impl Into<String>) -> Self {
        Self { reason: Some(reason.into()), ..Self::default() }
    }

    pub fn with_context(
        from_addr: impl Into<String>,
        to_addr: impl Into<String>,
        chain_code: impl Into<String>,
        token_addr: impl Into<String>,
        value: impl Into<String>,
        balance: impl Into<String>,
        need: impl Into<String>,
        fee: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            from_addr: Some(from_addr.into()),
            to_addr: Some(to_addr.into()),
            chain_code: Some(chain_code.into()),
            token_addr: Some(token_addr.into()),
            value: Some(value.into()),
            balance: Some(balance.into()),
            need: Some(need.into()),
            fee: Some(fee.into()),
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ChainError {
    #[error("Chain not found: {0}")]
    NotFound(String),
    #[error("Insufficient balance{0}")]
    InsufficientBalance(InsufficientBalanceDetail),
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
    #[error("last withdraw time  less than 24 hours")]
    WithdrawTooSoon,
    #[error("witnessAccount does not have any reward")]
    WitnessAccountDoesNotHaveAnyReward,
    #[error("The lock period for this time cannot be less than the remaining time")]
    LockPeriodTooShort,
    #[error("ApproveRepeated")]
    ApproveRepeated,
    #[error("ApproveCanceling")]
    ApproveCanceling,
    // 链兑换相关的错误
    #[error("SwapSimulate error:{0}")]
    SwapSimulate(String),
    #[error("time error:{0}")]
    SolSwapTime(String),

    #[error("Invalid raw transaction")]
    InvalidRawTx,
}

impl ChainError {
    pub fn insufficient_balance() -> Self {
        ChainError::InsufficientBalance(InsufficientBalanceDetail::default())
    }

    pub fn insufficient_balance_with_detail(detail: InsufficientBalanceDetail) -> Self {
        ChainError::InsufficientBalance(detail)
    }

    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            ChainError::NotFound(_) => 3501,
            ChainError::InsufficientBalance(_) => 3502,
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
            ChainError::WithdrawTooSoon => 3516,
            ChainError::WitnessAccountDoesNotHaveAnyReward => 3517,
            ChainError::LockPeriodTooShort => 3518,
            ChainError::ApproveRepeated => 3519,
            ChainError::ApproveCanceling => 3520,
            ChainError::SwapSimulate(_) => 3521,
            ChainError::SolSwapTime(_) => 3522,

            ChainError::InvalidRawTx => 3523,
        }
    }
}
