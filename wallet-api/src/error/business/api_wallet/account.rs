#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    // 无法移除已配置出款策略的账户
    #[error("An account configured with a withdrawal strategy cannot be removed")]
    ConfiguredWithdrawalStrategyAccountCantBeRemoved,
    #[error("Expand address not done yet")]
    ExpandAddressNotDoneYet,
}

impl AccountError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            AccountError::ConfiguredWithdrawalStrategyAccountCantBeRemoved => 20100,
            AccountError::ExpandAddressNotDoneYet => 20101,
        }
    }
}
