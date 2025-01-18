#[derive(Debug, thiserror::Error)]
pub enum BillError {
    #[error("Bill not found")]
    NotFound,
    #[error("exists uncomfirmation tx")]
    ExistsUncomfrimationTx,
}

impl BillError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            BillError::NotFound => 3301,
            BillError::ExistsUncomfrimationTx => 3302,
        }
    }
}
