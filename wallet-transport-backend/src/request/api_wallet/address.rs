#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadAllocatedAddressesReq {
    wallet_address: String,
    addresses: Vec<String>,
}

impl UploadAllocatedAddressesReq {
    pub fn new(wallet_address: &str, addresses: Vec<String>) -> Self {
        Self { wallet_address: wallet_address.to_string(), addresses }
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

#[derive(Debug, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AddressListReq {
    pub uid: String,
    pub chain_code: String,
    pub page_num: i32,
    pub page_size: i32,
}

impl AddressListReq {
    pub fn new(uid: &str, chain_code: &str, page_num: i32, page_size: i32) -> Self {
        Self { uid: uid.to_string(), chain_code: chain_code.to_string(), page_num, page_size }
    }
}
