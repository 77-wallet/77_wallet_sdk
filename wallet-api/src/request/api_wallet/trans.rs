use std::fmt;

use wallet_database::entities::asset_token_key::AssetTokenKey;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiWithdrawReq {
    pub uid: String, // 钱包
    pub from: String,
    pub to: String,
    pub value: String,
    pub validate: String,
    #[serde(rename = "chain")]
    pub chain_code: String,
    #[serde(rename = "token_addr")]
    pub token_address: AssetTokenKey,
    #[serde(rename = "token_code")]
    pub symbol: String,
    pub trade_no: String,
    // 交易类型： 1 提币 / 2 归集
    pub trade_type: u8,
    pub audit: u32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiTransferFeeReq {
    pub uid: String, // 钱包
    pub from: String,
    pub to: String,
    pub value: String,
    pub validate: String,
    #[serde(rename = "chain")]
    pub chain_code: String,
    #[serde(rename = "token_addr")]
    pub token_address: AssetTokenKey,
    #[serde(rename = "token_code")]
    pub symbol: String,
    pub trade_no: String,
    // 交易类型： 1 提币 / 2 归集
    pub trade_type: u8,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiCollectReq {
    pub uid: String, // 钱包
    pub from: String,
    pub to: String,
    pub value: String,
    pub validate: String,
    #[serde(rename = "chain")]
    pub chain_code: String,
    #[serde(rename = "token_addr")]
    pub token_address: AssetTokenKey,
    #[serde(rename = "token_code")]
    pub symbol: String,
    pub trade_no: String,
    // 交易类型： 1 提币 / 2 归集
    pub trade_type: u8,
    pub risk_addr: u8,
}

#[derive(Debug, Clone)]
pub struct ApiBaseTransferReq {
    pub from: String,
    pub to: String,
    pub value: String,
    pub chain_code: String,
    pub token_address: AssetTokenKey,
    pub decimals: u8,
    pub symbol: String,
    // 用户后端回收资源的id
    pub request_resource_id: Option<String>,
    // pub address_type: Option<String>,
    pub spend_all: bool,
    pub notes: Option<String>,
    // 认为每个必的交易参数不完全相同，每个币的adapter自行解析特殊参数
    pub metadata: Option<String>,
}

impl ApiBaseTransferReq {
    pub fn new(from: &str, to: &str, value: &str, chain_code: &str) -> Self {
        Self {
            from: from.to_string(),
            to: to.to_string(),
            value: value.to_string(),
            chain_code: chain_code.to_string(),
            token_address: AssetTokenKey::Native,
            decimals: 0,
            symbol: "".to_string(),
            request_resource_id: None,

            // address_type: None,
            spend_all: false,
            notes: None,
            metadata: None,
        }
    }

    pub fn with_token(&mut self, token_key: impl Into<AssetTokenKey>, decimals: u8, symbol: &str) {
        self.token_address = token_key.into();
        self.decimals = decimals;
        self.symbol = symbol.to_string();
    }

    // pub fn with_request_resource_id(&mut self, request_resource_id: Option<String>) {
    //     self.request_resource_id = request_resource_id
    // }

    // pub fn with_spend_all(&mut self, spend_all: bool) {
    //     self.spend_all = spend_all;
    // }

    // pub fn with_notes(&mut self, notes: String) {
    //     self.notes = Some(notes);
    // }
}

#[derive(Clone)]
pub struct ApiTransferReq {
    pub base: ApiBaseTransferReq,
    pub password: String,
    pub nonce: u64,
}

impl fmt::Debug for ApiTransferReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiTransferReq")
            .field("base", &self.base)
            .field("password", &"<redacted>")
            .field("nonce", &self.nonce)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiBaseTransferReq, ApiTransferReq};
    use wallet_database::entities::asset_token_key::AssetTokenKey;

    #[test]
    fn api_transfer_req_debug_redacts_password() {
        let req = ApiTransferReq {
            base: ApiBaseTransferReq {
                from: "from".to_string(),
                to: "to".to_string(),
                value: "1".to_string(),
                chain_code: "eth".to_string(),
                token_address: AssetTokenKey::Native,
                decimals: 18,
                symbol: "ETH".to_string(),
                request_resource_id: Some("request-1".to_string()),
                spend_all: false,
                notes: Some("note".to_string()),
                metadata: Some("meta".to_string()),
            },
            password: "super-secret".to_string(),
            nonce: 42,
        };

        let debug = format!("{req:?}");
        assert!(!debug.contains("super-secret"));
        assert!(debug.contains("<redacted>"));
    }
}
