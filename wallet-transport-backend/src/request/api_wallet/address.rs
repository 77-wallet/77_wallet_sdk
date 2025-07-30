#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOrUpdateCollectionStrategyReq {}

impl SaveOrUpdateCollectionStrategyReq {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadAllocatedAddressesReq {
    wallet_address: String,
    addresses: Vec<String>,
}

impl UploadAllocatedAddressesReq {
    pub fn new(wallet_address: &str, addresses: Vec<String>) -> Self {
        Self {
            wallet_address: wallet_address.to_string(),
            addresses,
        }
    }
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreAddressesReq {}

impl RestoreAddressesReq {
    pub fn new() -> Self {
        Self {}
    }
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsedAddressListReq {}

impl UsedAddressListReq {
    pub fn new() -> Self {
        Self {}
    }
}
