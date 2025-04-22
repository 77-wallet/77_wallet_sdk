#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnconfirmMsgRes {
    pub list: Vec<String>,
}
