use crate::request::api_wallet::audit::*;

use super::BackendApi;

impl BackendApi {
    // 交易记录恢复
    pub async fn report_audit_result(
        &self,
        req: &AuditResultReportReq,
    ) -> Result<Option<()>, crate::Error> {
        todo!()
    }
}
