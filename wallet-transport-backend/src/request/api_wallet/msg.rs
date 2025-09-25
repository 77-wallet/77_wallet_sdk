#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MsgAckReq(pub Vec<MsgAckItem>);

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MsgAckItem {
    msg_id: String,
}

impl MsgAckReq {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, msg_id: &str) {
        self.0.push(MsgAckItem { msg_id: msg_id.to_string() });
    }
}

impl Default for MsgAckReq {
    fn default() -> Self {
        Self::new()
    }
}
