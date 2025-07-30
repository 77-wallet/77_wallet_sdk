pub mod address;
pub mod audit;
pub mod strategy;
pub mod transaction;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletBindAppIdReq {}

impl WalletBindAppIdReq {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletUnbindAppIdReq {}

impl WalletUnbindAppIdReq {
    pub fn new() -> Self {
        Self {}
    }
}
