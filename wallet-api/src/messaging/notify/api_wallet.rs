#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawFront {
    pub uid: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawNoPassFront {
    pub uid: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectFeeNotEnoughFront {
    pub uid: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeFront {
    pub uid: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
}