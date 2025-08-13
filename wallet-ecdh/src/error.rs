use thiserror::Error;

#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("加密失败: {0}")]
    EncryptionFailed(String),
    #[error("解密失败: {0}")]
    DecryptionFailed(String),
    #[error("密钥派生失败: {0}")]
    KeyDerivationFailed(String),
    #[error("无效的加密数据")]
    InvalidEncryptedData,
}