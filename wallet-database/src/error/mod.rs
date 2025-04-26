pub mod database;
pub use database::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] database::DatabaseError),
    #[error("Utils error: {0}")]
    Utils(#[from] wallet_utils::Error),
    #[error("{0}")]
    Other(String),
    #[error("data Not Found: {0}")]
    NotFound(String),
}

impl Error {
    pub fn is_network_error(&self) -> bool {
        match self {
            Error::Utils(e) => e.is_network_error(),
            _ => false,
        }
    }
}
