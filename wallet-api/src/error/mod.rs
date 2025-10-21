pub mod business;
pub mod service;
pub mod system;

#[derive(Debug, thiserror::Error)]
pub enum Errors {
    #[error("parameter error: {0}")]
    Parameter(String),
    #[error("service error: {0}")]
    Service(#[from] service::ServiceError),
}

impl From<Errors> for (i64, String) {
    fn from(err: Errors) -> Self {
        let (code, message) = match err {
            Errors::Service(e) => e.into(),
            Errors::Parameter(_) => (204, err.to_string()),
        };
        (code, message)
    }
}
