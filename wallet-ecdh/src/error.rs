use thiserror::Error;

#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("invalid key")]
    InvalidKey,
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("key derivation failed: {0}")]
    KeyDerivationFailed(String),
    #[error("invalid encryption key")]
    InvalidEncryptedData,
}