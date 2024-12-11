#[derive(Debug, thiserror::Error)]
pub enum ChainNodeError {
    #[error("chain not found")]
    ChainNotFound,
}

impl ChainNodeError {
    pub(crate) fn get_status_code(&self) -> u32 {
        match self {
            ChainNodeError::ChainNotFound => 4201,
        }
    }
}
