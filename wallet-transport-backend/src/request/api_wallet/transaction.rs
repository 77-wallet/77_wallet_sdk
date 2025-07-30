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
pub struct TxExecReceiptUploadReq {}

impl TxExecReceiptUploadReq {
    pub fn new() -> Self {
        Self {}
    }
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeCollectionUploadReq {}

impl FeeCollectionUploadReq {
    pub fn new() -> Self {
        Self {}
    }
}
