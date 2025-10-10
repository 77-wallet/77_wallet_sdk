use crate::request::api_wallet::audit::*;

use crate::{
    api::BackendApi, consts::endpoint::api_wallet::TRANS_AUDIT, response::BackendResponse,
};
use crate::api_request::ApiBackendRequest;
use crate::api_response::ApiBackendResponse;

impl BackendApi {
    // 交易记录恢复
    pub async fn report_audit_result(
        &self,
        req: &AuditResultReportReq,
    ) -> Result<Option<()>, crate::Error> {
        let api_req = ApiBackendRequest::new( req)?;

        let res = self.client.post(TRANS_AUDIT).json(api_req).send::<ApiBackendResponse>().await?;
        tracing::info!("res: {res:#?}");
        res.process()
    }
}
