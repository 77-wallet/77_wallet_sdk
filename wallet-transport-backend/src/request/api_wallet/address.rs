use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpandAddressReq {
    /// 发起方 (SDK / BACK)
    pub source: String,

    /// uid：只能是自账户 uid
    pub uid: String,

    /// 请求参数：key：chainCode -> value：Vec<AddressParam>
    pub param: HashMap<String, Vec<AddressParam>>,

    pub serial_no: Option<String>,
}

impl ExpandAddressReq {
    pub fn new_sdk(uid: &str) -> Self {
        Self {
            source: "SDK".to_string(),
            uid: uid.to_string(),
            param: HashMap::new(),
            serial_no: None,
        }
    }

    pub fn new_back(uid: &str, serial_no: &str) -> Self {
        Self {
            source: "BACK".to_string(),
            uid: uid.to_string(),
            param: HashMap::new(),
            serial_no: Some(serial_no.to_string()),
        }
    }

    pub fn add_chain_code(&mut self, chain_code: &str, address_param: AddressParam) {
        self.param.entry(chain_code.to_string()).or_insert_with(Vec::new).push(address_param);
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressParams(pub Vec<AddressParam>);

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressParam {
    /// 地址下标
    pub index: i32,

    /// 地址集合
    pub address_list: Vec<String>,
}

impl AddressParam {
    pub fn new(index: i32) -> Self {
        Self { index, address_list: Vec::new() }
    }

    pub fn push(&mut self, address: &str) {
        self.address_list.push(address.to_string());
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
