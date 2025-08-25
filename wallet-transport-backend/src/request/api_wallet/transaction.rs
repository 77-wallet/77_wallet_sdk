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
    hash: String,
    from: String,
    to: String,
    amount: f64,
}

impl FeeCollectionUploadReq {
    pub fn new(trade_no: &str, hash: &str, from: &str, to: &str, amount: f64) -> Self {
        Self {
            trade_no: trade_no.to_string(),
            hash: hash.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            amount,
        }
    }
}
