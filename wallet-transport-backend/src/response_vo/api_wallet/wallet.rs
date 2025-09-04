#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeysUidCheckRes {
    uid: String,
    /// NORMAL_WALLET / API_WALLET / NOT_FOUND
    status: String,
}

impl KeysUidCheckRes {
    pub fn new(uid: &str, status: &str) -> Self {
        Self { uid: uid.to_string(), status: status.to_string() }
    }
}
