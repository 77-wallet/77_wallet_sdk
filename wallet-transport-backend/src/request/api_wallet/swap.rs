
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiInitSwapReq {
    pub sn: String,
    pub client_pub_key : String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiInitSwapResponse {
    pub pub_key : String,
}