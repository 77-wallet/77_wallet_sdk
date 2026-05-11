#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDelegationApplyReq {
    pub uid: String,
    pub trade_no: String,
    pub origin_trade_no: String,
    #[serde(rename = "chain")]
    pub chain_code: String,
    pub receiver_address: String,
    #[serde(rename = "rscType")]
    pub resource_type: u32,
    pub amount: String,
    #[serde(rename = "tradeType")]
    pub trade_type: u32,
}

impl ResourceDelegationApplyReq {
    pub fn new(
        uid: &str,
        trade_no: &str,
        origin_trade_no: &str,
        chain_code: &str,
        receiver_address: &str,
        resource_type: u32,
        amount: &str,
        trade_type: u32,
    ) -> Self {
        Self {
            uid: uid.to_string(),
            trade_no: trade_no.to_string(),
            origin_trade_no: origin_trade_no.to_string(),
            chain_code: chain_code.to_string(),
            receiver_address: receiver_address.to_string(),
            resource_type,
            amount: amount.to_string(),
            trade_type,
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDelegationApplyResp {
    pub success: bool,
    pub trade_no: Option<String>,
    pub message: Option<String>,
}

impl ResourceDelegationApplyResp {
    pub fn is_success(&self) -> bool {
        self.success
    }
}
