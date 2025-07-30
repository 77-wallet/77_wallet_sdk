use wallet_database::repositories::ResourcesRepo;
use wallet_transport_backend::request::api_wallet::audit::AuditResultReportReq;

pub struct AuditService {
    pub repo: ResourcesRepo,
}

impl AuditService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo }
    }

    pub async fn audit_withdrawal_order(
        self,
        order_id: &str,
        status: i8,
    ) -> Result<(), crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;

        let req = AuditResultReportReq::new();
        backend_api.report_audit_result(&req).await?;

        Ok(())
    }
}
