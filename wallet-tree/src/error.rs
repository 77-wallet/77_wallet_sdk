#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("The wallet key is not saved locally")]
    LocalNoWallet,
    // #[error("parse failed: {0:?}")]
    // Parse(String),
    #[error("Tree error: `{0}`")]
    Utils(#[from] wallet_utils::Error),
    #[error("Filename invalid")]
    FilenameInvalid,
    #[error("Types error: `{0}`")]
    Types(#[from] wallet_types::Error),
}
