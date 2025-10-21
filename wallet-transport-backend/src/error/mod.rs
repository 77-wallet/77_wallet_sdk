use wallet_ecdh::error::EncryptionError;
use wallet_transport::errors::TransportError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Transport error: `{0}`")]
    Transport(#[from] TransportError),
    #[error("Backend error: `{0:?}`")]
    Backend(Option<String>),
    #[error("Utils error: `{0}`")]
    Utils(#[from] wallet_utils::error::Error),
    #[error("backend service error")]
    BackendServiceError(#[from] BackendServiceError),
    #[error("encryption error: `{0}`")]
    EncryptionError(#[from] EncryptionError),
}
impl Error {
    pub fn is_network_error(&self) -> bool {
        match self {
            Error::Transport(e) => e.is_network_error(),
            Error::Utils(e) => e.is_network_error(),
            _ => false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BackendServiceError {
    #[error("not platform address")]
    NotPlatformAddress,
}
