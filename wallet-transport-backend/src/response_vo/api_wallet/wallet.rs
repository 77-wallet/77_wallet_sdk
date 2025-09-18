#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeysUidCheckRes {
    pub uid: String,
    /// NORMAL_WALLET / API_WALLET / NOT_FOUND
    pub status: UidStatus,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UidStatus {
    NormalWallet,
    ApiWallet,
    NotFound,
}
