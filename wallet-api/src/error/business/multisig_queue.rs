#[derive(Debug, thiserror::Error)]
pub enum MultisigQueueError {
    #[error("Multisig Queue already exists")]
    AlreadyExist,
    #[error("Multisig Queue not found")]
    NotFound,
    #[error("the transaction has exec or sign not complete")]
    NotPendingExecStatus,
    #[error("multisig queue is expired")]
    Expired,
    #[error("has already been executed")]
    AlreadyExecuted,
}

impl MultisigQueueError {
    pub(crate) fn get_status_code(&self) -> u32 {
        match self {
            MultisigQueueError::AlreadyExist => 3700,
            MultisigQueueError::NotFound => 3701,
            MultisigQueueError::NotPendingExecStatus => 3702,
            MultisigQueueError::Expired => 3703,
            MultisigQueueError::AlreadyExecuted => 3704,
        }
    }
}
