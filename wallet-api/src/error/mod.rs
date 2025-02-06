pub(crate) mod business;
pub mod service;
pub use service::ServiceError;
pub(crate) mod system;

#[derive(Debug, thiserror::Error)]
pub enum Errors {
    #[error("parameter error: {0}")]
    Parameter(String),
    #[error("service error: {0}")]
    Service(#[from] service::ServiceError),
}
