#[derive(Debug, thiserror::Error)]
pub enum ChainNodeError {
    #[error("chain not found")]
    ChainNotFound,
    #[error("{0}")]
    InvalidParams(String),
}

impl ChainNodeError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            ChainNodeError::ChainNotFound => 4201,
            ChainNodeError::InvalidParams(_) => 4202,
        }
    }
}
