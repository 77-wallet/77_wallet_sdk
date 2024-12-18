// biz_type = ERR
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrFront {
    pub event: String,
    pub message: String,
}

// biz_type = DEBUG
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugFront {
    pub message: serde_json::Value,
}

// biz_type = ORDER_MULTI_SIGN_ACCEPT_COMPLETE_MSG
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionErrorFrontend {
    pub message: String,
}

// 执行交易的过程给前端发送交易的类型
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionProcessFrontend {
    // 交易的类型,对比账单的交易类型
    pub tx_kind: i8,
    // 第几笔交易,默认不需要在批量执行的时候需要
    pub tx_num: Option<i32>,
    pub process: Process,
}
impl TransactionProcessFrontend {
    pub fn new(tx_kind: i8, process: Process) -> Self {
        Self {
            tx_kind,
            tx_num: None,
            process,
        }
    }
    pub fn new_with_num(tx_kind: i8, tx_num: i32, process: Process) -> Self {
        Self {
            tx_kind,
            tx_num: Some(tx_num),
            process,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum Process {
    Building,
    Broadcast,
}
