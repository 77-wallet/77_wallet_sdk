use crate::request::api_wallet::audit::*;

use crate::api::BackendApi;
use crate::consts::endpoint::api_wallet::TRANS_AUDIT;
use crate::response::BackendResponse;

impl BackendApi {
    // 交易记录恢复
    pub async fn report_audit_result(
        &self,
        req: &AuditResultReportReq,
    ) -> Result<Option<()>, crate::Error> {
        let req = serde_json::json!(req);
        tracing::info!("req: {}", req.to_string());

        let res = self.client.post(TRANS_AUDIT).json(req).send::<BackendResponse>().await?;
        tracing::info!("res: {res:#?}");
        res.process(&self.aes_cbc_cryptor)
    }
}
