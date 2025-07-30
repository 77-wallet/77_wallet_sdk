#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditResultReportReq {}

impl AuditResultReportReq {
    pub fn new() -> Self {
        Self {}
    }
}
