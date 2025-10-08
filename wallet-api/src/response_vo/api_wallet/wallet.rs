use wallet_database::entities::api_wallet::ApiWalletEntity;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWalletInfo {
    pub address: String,
    pub uid: String,
    pub name: String,
    pub app_id: String,
}

impl From<&ApiWalletEntity> for ApiWalletInfo {
    fn from(e: &ApiWalletEntity) -> Self {
        Self {
            address: e.address.clone(),
            uid: e.uid.clone(),
            name: e.name.clone(),
            app_id: e.app_id.clone(),
        }
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
    pub recharge_wallet: Option<ApiWalletInfo>,
    pub withdraw_wallet: Option<ApiWalletInfo>,
}
