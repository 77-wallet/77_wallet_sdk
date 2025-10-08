use serde::Deserialize;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MsgAckItem {
    msg_id: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MsgAckReq(pub Vec<MsgAckItem>);

impl MsgAckReq {
    pub fn push(&mut self, msg_id: &str) {
        self.0.push(MsgAckItem { msg_id: msg_id.to_string() });
    }
}
