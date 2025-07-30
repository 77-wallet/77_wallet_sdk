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
pub struct UploadAllocatedAddressesReq {}

impl UploadAllocatedAddressesReq {
    pub fn new() -> Self {
        Self {}
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
