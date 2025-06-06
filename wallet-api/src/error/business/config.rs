#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config not found: {0}")]
    NotFound(String),
    #[error("Keys not reset")]
    KeysNotReset,
}

impl ConfigError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            ConfigError::NotFound(_) => 4300,
            ConfigError::KeysNotReset => 4301,
        }
    }
}
