use wallet_database::entities::api_withdraw::ApiWithdrawEntity;
use wallet_database::repositories::ResourcesRepo;
use wallet_database::repositories::api_withdraw::ApiWithdrawRepo;
use wallet_transport_backend::request::api_wallet::audit::AuditResultReportReq;

pub struct WithdrawService {
    pub repo: ResourcesRepo,
}

impl WithdrawService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo }
    }

    pub async fn get_withdraw_order_list(
        &self,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiWithdrawRepo::list_api_withdraw(&pool)
            .await
            .map_err(|e| e.into())
    }

    pub async fn sign_withdrawal_order(
        &self,
        order_id: &str,
        status: i8,
    ) -> Result<(), crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;

        let req = AuditResultReportReq::new();
        backend_api.report_audit_result(&req).await?;

        Ok(())
    }

    pub async fn reject_withdrawal_order(
        &self,
        order_id: &str,
        status: i8,
    ) -> Result<(), crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;

        let req = AuditResultReportReq::new();
        backend_api.report_audit_result(&req).await?;

        Ok(())
    }
}
