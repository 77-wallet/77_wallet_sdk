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

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryWalletActivationInfoResp(pub Vec<QueryWalletActivationInfoRespItem>);

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryWalletActivationInfoRespItem {
    pub chain: String,
    pub active: ActiveStatus,
}

#[derive(Debug, serde_repr::Deserialize_repr, serde_repr::Serialize_repr)]
#[repr(u8)]
pub enum ActiveStatus {
    Active = 1,
    Inactive = 0,
}
