use wallet_database::entities::api_wallet::ApiWalletEntity;

use crate::response_vo::account::BalanceInfo;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletInfo {
    pub address: String,
    pub uid: String,
    pub name: String,
    pub app_id: Option<String>,
    pub balance: BalanceInfo,
}

impl From<&ApiWalletEntity> for WalletInfo {
    fn from(e: &ApiWalletEntity) -> Self {
        Self {
            address: e.address.clone(),
            uid: e.uid.clone(),
            name: e.name.clone(),
            app_id: Some(e.app_id.clone()),
            balance: BalanceInfo::default(),
        }
    }
}

impl WalletInfo {
    pub(crate) fn with_balance(mut self, balance: BalanceInfo) -> Self {
        self.balance = balance;
        self
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWalletList(pub Vec<ApiWalletItem>);

impl std::ops::Deref for ApiWalletList {
    type Target = Vec<ApiWalletItem>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ApiWalletList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ApiWalletList {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn push(&mut self, item: ApiWalletItem) {
        self.0.push(item);
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWalletItem {
    pub recharge_wallet: Option<WalletInfo>,
    pub withdraw_wallet: Option<WalletInfo>,
}
