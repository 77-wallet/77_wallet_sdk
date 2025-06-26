#[derive(Debug, thiserror::Error)]
pub enum ChainNodeError {
    #[error("chain not found")]
    ChainNotFound,
    #[error("node not found")]
    NodeNotFound,
}

impl ChainNodeError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            ChainNodeError::ChainNotFound => 4201,
            ChainNodeError::NodeNotFound => 4202,
        }
    }
}
