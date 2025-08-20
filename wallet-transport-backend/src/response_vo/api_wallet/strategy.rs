#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionStrategyResp {
    pub min_amount: f64,
    pub normal_address: String,
    pub normal_index: i32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawStrategyResp {
    pub min_amount: f64,
    pub normal_address: String,
    pub normal_index: i32,
}
