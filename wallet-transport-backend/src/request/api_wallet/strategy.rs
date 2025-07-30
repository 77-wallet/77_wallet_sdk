#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOrUpdateCollectionStrategyReq {}

impl SaveOrUpdateCollectionStrategyReq {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOrUpdateWithdrawStrategyReq {}

impl SaveOrUpdateWithdrawStrategyReq {
    pub fn new() -> Self {
        Self {}
    }
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryCollectionStrategyReq {}

impl QueryCollectionStrategyReq {
    pub fn new() -> Self {
        Self {}
    }
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryWithdrawStrategyReq {}

impl QueryWithdrawStrategyReq {
    pub fn new() -> Self {
        Self {}
    }
}
