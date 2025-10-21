use crate::request::api_wallet::audit::*;
use wallet_ecdh::GLOBAL_KEY;

use crate::{
    api::BackendApi, api_request::ApiBackendRequest, api_response::ApiBackendResponse,
    consts::endpoint::api_wallet::TRANS_AUDIT, response::BackendResponse,
};

impl BackendApi {
    // 交易记录恢复
    pub async fn report_audit_result(
        &self,
        req: &AuditResultReportReq,
    ) -> Result<Option<()>, crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        let api_req = ApiBackendRequest::new(req)?;
        let res = self.client.post(TRANS_AUDIT).json(api_req).send::<ApiBackendResponse>().await?;
        tracing::info!("res: {res:#?}");
        res.process(TRANS_AUDIT)
    }
}
