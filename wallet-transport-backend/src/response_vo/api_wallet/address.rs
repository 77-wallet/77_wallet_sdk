use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsedAddressListResp {
    pub total_elements: i64,
    pub total_pages: i64,
    pub first: bool,
    pub last: bool,
    pub size: i64,
    pub number: i32,
    pub sort: Sort,
    pub pageable: Pageable,
    pub number_of_elements: i64,
    pub empty: bool,
    pub content: Vec<Item>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// 下标
    pub index: i32,
    /// 地址绑定状态： 0 未绑定 / 1 已绑定
    pub bind_status: u8,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sort {
    pub empty: bool,
    pub unsorted: bool,
    pub sorted: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pageable {
    pub offset: i64,
    pub sort: Sort,
    pub page_number: i64,
    pub page_size: i64,
    pub unpaged: bool,
    pub paged: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetsListRes(pub Vec<AssetsItem>);

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetsItem {
    pub index: i32,
    pub address_list: Vec<AddressItem>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressItem {
    pub address: String,
    #[serde(default)]
    pub token_infos: Vec<TokenInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    #[serde(rename = "tokenCode")]
    pub symbol: String,
    pub token_address: String,
    pub amount: f64,
    #[serde(rename = "assets")]
    pub usdt_amount: f64,
}
