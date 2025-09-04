#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreTxRecordsReq {}

impl RestoreTxRecordsReq {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MsgReceiptUploadReq {}

impl MsgReceiptUploadReq {
    pub fn new() -> Self {
        Self {}
    }
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxExecReceiptUploadReq {
    trade_no: String,
    #[serde(rename = "type")]
    typ: TransType,
    hash: String,
    status: TransStatus,
    remark: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TransType {
    /// Collection
    Col,
    /// Withdraw
    Wd,
    /// Fee
    Fee,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TransStatus {
    /// Success
    Success,
    /// Fail
    Fail,
}

impl TxExecReceiptUploadReq {
    pub fn new(
        trade_no: &str,
        typ: TransType,
        hash: &str,
        status: TransStatus,
        remark: &str,
    ) -> Self {
        Self {
            trade_no: trade_no.to_string(),
            typ,
            hash: hash.to_string(),
            status,
            remark: remark.to_string(),
        }
    }
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeCollectionUploadReq {
    trade_no: String,
    from: String,
    to: String,
    amount: f64,
    chain_code: String,
    #[serde(rename = "token_code")]
    symbol: String,
    #[serde(rename = "contractAddress")]
    token_address: String,
}

impl FeeCollectionUploadReq {
    pub fn new(
        trade_no: &str,
        chain_code: &str,
        symbol: &str,
        token_address: &str,
        from: &str,
        to: &str,
        amount: f64,
    ) -> Self {
        Self {
            trade_no: trade_no.to_string(),
            chain_code: chain_code.to_string(),
            symbol: symbol.to_string(),
            token_address: token_address.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            amount,
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventAcceptTxReq {
    pub trade_no: String,
    #[serde(rename = "type")]
    pub typ: TransType,
    pub ack_type: String,
}

impl EventAcceptTxReq {
    pub fn new(trade_no: &str, typ: TransType, ack_type: &str) -> Self {
        Self { trade_no: trade_no.to_string(), typ, ack_type: ack_type.to_string() }
    }
}