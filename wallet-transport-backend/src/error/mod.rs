use wallet_ecdh::error::EncryptionError;
use wallet_transport::errors::{RetryPolicy, TransportError};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Transport error: `{0}`")]
    Transport(#[from] TransportError),
    #[error("Backend error: `{0:?}`")]
    Backend(Option<String>),
    #[error("Backend error: `{0:?}`")]
    ApiBackend(i64, Option<String>),
    #[error("Utils error: `{0}`")]
    Utils(#[from] wallet_utils::error::Error),
    #[error("backend service error")]
    BackendServiceError(#[from] BackendServiceError),
    #[error("encryption error: `{0}`")]
    EncryptionError(#[from] EncryptionError),
    #[error("acquire error: `{0}`")]
    AcquireError(#[from] tokio::sync::AcquireError),
    #[error("rate limited")]
    RateLimited,
    #[error("backpressure")]
    Backpressure,
    #[error("timeout")]
    Timeout,
}
impl Error {
    pub fn is_network_error(&self) -> bool {
        match self {
            Error::Transport(e) => e.is_network_error(),
            Error::Utils(e) => e.is_network_error(),
            _ => false,
        }
    }

    pub fn retry_policy(&self) -> RetryPolicy {
        match self {
            Error::Transport(e) => e.retry_policy(),
            Error::ApiBackend(code, _) if *code == 429 => RetryPolicy::Delay,
            Error::RateLimited => RetryPolicy::Delay,
            _ => RetryPolicy::Never,
        }
    }

    pub fn is_rate_limited(&self) -> bool {
        match self {
            crate::Error::ApiBackend(code, _) if *code == 429 => true,
            crate::Error::Transport(msg) => match msg {
                TransportError::NodeResponseError(err) if err.code == 429 => true,
                _ => false,
            },
            _ => false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BackendServiceError {
    #[error("not platform address")]
    NotPlatformAddress,
}
