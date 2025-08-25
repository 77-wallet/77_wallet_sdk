#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiTransReq {
    pub from: String,
    pub to: String,
    pub value: String,
    #[serde(rename = "chain")]
    pub chain_code: String,
    #[serde(rename = "token_addr")]
    pub token_address: String,
    #[serde(rename = "token_code")]
    pub symbol: String,
    pub trade_no: String,
    // 交易类型： 1 提币 / 2 归集
    pub trade_type: u8,
    pub uid: String,
}