#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeysUidCheckRes {
    pub uid: String,
    pub status: UidStatus,
}

impl KeysUidCheckRes {
    pub fn is_normal_wallet(&self) -> bool {
        self.status == UidStatus::NormalWallet
    }

    pub fn is_api_wallet(&self) -> bool {
        self.status == UidStatus::ApiRAW || self.status == UidStatus::ApiWAW
    }

    pub fn is_sub_account_wallet(&self) -> bool {
        self.status == UidStatus::ApiRAW
    }

    pub fn is_withdrawal_wallet(&self) -> bool {
        self.status == UidStatus::ApiWAW
    }
}

// #[derive(Debug, serde_repr::Deserialize_repr, serde_repr::Serialize_repr, PartialEq, Eq)]
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
// #[repr(u8)]
pub enum UidStatus {
    /// 普通钱包
    NormalWallet,
    /// API钱包-收款钱包
    ApiRAW,
    /// API钱包-出款钱包
    ApiWAW,
    /// 4 UID不存在
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

#[derive(Debug, serde_repr::Deserialize_repr, serde_repr::Serialize_repr, Clone)]
#[repr(u8)]
pub enum ActiveStatus {
    Active = 1,
    Inactive = 0,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryUidBindInfoRes {
    pub sn: String,
    pub app_id: String,
    /// 商户id
    pub org_id: String,
    pub bind_status: bool,
}
