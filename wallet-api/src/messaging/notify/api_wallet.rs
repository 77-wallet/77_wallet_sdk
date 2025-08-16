#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawFront {
    pub event: String,
    pub message: String,
}
