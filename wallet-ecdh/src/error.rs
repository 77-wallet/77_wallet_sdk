use k256::ecdsa::Error;
use k256::elliptic_curve;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("lock key")]
    LockPoisoned,
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
    #[error("signature err")]
    SignatureError(#[from] Error),
    #[error("signature err")]
    EllipticCurveError(#[from] elliptic_curve::Error),
    #[error("wallet utils err")]
    WalletUtilsError(#[from] wallet_utils::Error),
}