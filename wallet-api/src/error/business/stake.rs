#[derive(Debug, thiserror::Error)]
pub enum StakeError {
    //  平台没有开启 补贴功能
    #[error("delegate switch close")]
    SwitchClose,
    #[error("User resources are sufficient, no need for additional resources")]
    ResourceSufficient,
    #[error("TRX balance is sufficient, subsidy condition not met")]
    TrxSufficient,
    #[error("Energy is sufficient, subsidy condition not met")]
    EnergySufficient,
    #[error("Failed to delegate TRX")]
    DelegateTrxFailed,
    #[error("Failed to delegate energy")]
    DelegateEnergyFailed,
}

impl StakeError {
    pub(crate) fn get_status_code(&self) -> u32 {
        match self {
            StakeError::SwitchClose => 3900,
            StakeError::ResourceSufficient => 3901,
            StakeError::TrxSufficient => 3902,
            StakeError::EnergySufficient => 3903,
            StakeError::DelegateTrxFailed => 3904,
            StakeError::DelegateEnergyFailed => 3905,
        }
    }
}
