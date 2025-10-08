use wallet_database::entities::api_wallet::ApiWalletType;

use crate::response_vo::wallet::ChainInfo;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiAccountInfo {
    pub account_id: u32,
    pub account_index_map: wallet_utils::address::AccountIndexMap,
    pub name: String,
    pub balance: crate::response_vo::account::BalanceInfo,
    pub chain: Vec<ChainInfo>,
    pub api_wallet_type: ApiWalletType,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiAccountInfos(pub Vec<ApiAccountInfo>);

impl ApiAccountInfos {
    pub(crate) fn new() -> Self {
        Self(Vec::new())
    }
}

impl std::ops::Deref for ApiAccountInfos {
    type Target = Vec<ApiAccountInfo>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ApiAccountInfos {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
