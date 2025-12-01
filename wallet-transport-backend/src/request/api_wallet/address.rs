use crate::request::AddressBatchInitReq;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiAddressInitReq {
    pub address_list: AddressBatchInitReq,
    pub batch_id: Option<String>,
}

impl ApiAddressInitReq {
    pub fn new() -> Self {
        Self { address_list: AddressBatchInitReq::new(), batch_id: None }
    }

    pub fn with_batch_id(self, batch_id: &str) -> Self {
        Self { batch_id: Some(batch_id.to_string()), ..self }
    }

    pub fn has_account(&self, input_index: i32) -> bool {
        self.address_list.0.iter().any(|a| a.index == input_index)
    }
}

// #[derive(Debug, serde::Serialize, serde::Deserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct ApiAddressInitReq {
//     pub address_list: AddressBatchInitReq,
// }

// impl ApiAddressInitReq {
//     pub fn new() -> Self {
//         Self { address_list: AddressBatchInitReq::new() }
//     }
// }

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
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

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AssetListReq {
    pub uid: String,
    pub chain_code: String,
    pub index_list: Vec<i32>,
}

impl AssetListReq {
    pub fn new(uid: &str, chain_code: &str, index_list: Vec<i32>) -> Self {
        Self { uid: uid.to_string(), chain_code: chain_code.to_string(), index_list }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpandAddressCompleteReq {
    uid: String,
    batch_id: String,
    serial_no: String,
    /// 处理结果
    status: bool,
    /// 备注
    remark: Option<String>,
}

impl ExpandAddressCompleteReq {
    pub fn new(
        uid: &str,
        batch_id: &str,
        serial_no: &str,
        status: bool,
        remark: Option<&str>,
    ) -> Self {
        Self {
            uid: uid.to_string(),
            batch_id: batch_id.to_string(),
            serial_no: serial_no.to_string(),
            status,
            remark: remark.map(|r| r.to_string()),
        }
    }
}
