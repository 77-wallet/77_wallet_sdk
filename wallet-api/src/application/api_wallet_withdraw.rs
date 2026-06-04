use crate::{
    context::Context, domain::api_wallet::trans::withdraw::ApiWithdrawDomain,
    error::service::ServiceError,
};
use wallet_database::{
    entities::api_trade_type::ApiTradeType, repositories::api_wallet::withdraw::ApiWithdrawRepo,
};
use wallet_transport_backend::request::api_wallet::audit::AuditResultReportReq;

pub(crate) struct ApiWithdrawApplication {
    ctx: &'static Context,
}

impl ApiWithdrawApplication {
    pub(crate) fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub(crate) async fn sign_withdrawal_order(&self, trade_no: &str) -> Result<(), ServiceError> {
        if !self.should_report_audit_result(trade_no).await? {
            return Ok(());
        }

        self.report_audit_result(trade_no, true, "OK").await?;
        ApiWithdrawDomain::sign_withdrawal_order(self.ctx, trade_no).await
    }

    pub(crate) async fn reject_withdrawal_order(&self, trade_no: &str) -> Result<(), ServiceError> {
        if !self.should_report_audit_result(trade_no).await? {
            return Ok(());
        }

        self.report_audit_result(trade_no, false, "user rejected").await?;
        ApiWithdrawDomain::reject_withdrawal_order(self.ctx, trade_no).await
    }

    async fn should_report_audit_result(&self, trade_no: &str) -> Result<bool, ServiceError> {
        let pool = self.ctx.api_transaction_pool()?;
        let entity =
            ApiWithdrawRepo::get_api_withdraw_by_trade_no(&pool, trade_no, ApiTradeType::Withdraw)
                .await
                .map_err(ServiceError::from)?;
        let should_report = ApiWithdrawDomain::has_no_audit_decision(&entity);
        if !should_report {
            tracing::info!(
                trade_no = %trade_no,
                audit_passed = entity.audit_passed_at.is_some(),
                audit_rejected = entity.audit_rejected_at.is_some(),
                "skip duplicate api withdraw audit report"
            );
        }
        Ok(should_report)
    }

    async fn report_audit_result(
        &self,
        trade_no: &str,
        result: bool,
        remark: &str,
    ) -> Result<(), ServiceError> {
        let backend_api = self.ctx.get_global_backend_api();
        let req = AuditResultReportReq::new(trade_no.to_string(), result, remark);
        backend_api.report_audit_result(&req).await?;
        Ok(())
    }
}
