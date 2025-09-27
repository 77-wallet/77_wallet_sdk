#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeysUidCheckRes {
    pub uid: String,
    pub status: UidStatus,
}

#[derive(Debug, serde_repr::Deserialize_repr, serde_repr::Serialize_repr, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(u8)]
pub enum UidStatus {
    /// 1 普通钱包
    NormalWallet = 1,
    /// 2 API钱包-收款钱包
    ApiRAW = 2,
    /// 3 API钱包-出款钱包
    ApiWAW = 3,
    /// 4 UID不存在
    NotFound = 4,
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

#[derive(Debug, serde_repr::Deserialize_repr, serde_repr::Serialize_repr, Clone)]
#[repr(u8)]
pub enum ActiveStatus {
    Active = 1,
    Inactive = 0,
}
