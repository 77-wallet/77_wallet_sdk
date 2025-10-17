use crate::{
    context::Context, domain::api_wallet::trans::withdraw::ApiWithdrawDomain,
    error::service::ServiceError, request::api_wallet::trans::ApiWithdrawReq,
};
use wallet_database::{
    entities::api_withdraw::ApiWithdrawEntity, pagination::Pagination,
    repositories::api_wallet::withdraw::ApiWithdrawRepo,
};
use wallet_transport_backend::request::api_wallet::audit::AuditResultReportReq;

pub struct WithdrawService {
    ctx: &'static Context,
}

impl WithdrawService {
    pub fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub async fn list_withdraw_order(
        &self,
        uid: &str,
    ) -> Result<Vec<ApiWithdrawEntity>, ServiceError> {
        let pool = self.ctx.get_global_sqlite_pool()?;
        ApiWithdrawRepo::list_api_withdraw(&pool, uid).await.map_err(|e| e.into())
    }

    pub async fn page_withdraw_order(
        &self,
        uid: &str,
        status: Option<u8>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, ServiceError> {
        let pool = self.ctx.get_global_sqlite_pool()?;
        ApiWithdrawRepo::page_api_withdraw(&pool, uid, status, page, page_size)
            .await
            .map_err(|e| e.into())
    }

    pub async fn withdrawal_order(
        &self,
        from: &str,
        to: &str,
        value: &str,
        validate: &str,
        chain_code: &str,
        token_address: Option<String>,
        symbol: &str,
        trade_no: &str,
        trade_type: u8,
        uid: &str,
        audit: u32,
    ) -> Result<(), ServiceError> {
        let req = ApiWithdrawReq {
            from: from.to_string(),
            to: to.to_string(),
            value: value.to_string(),
            validate: validate.to_string(),
            chain_code: chain_code.to_string(),
            token_address,
            symbol: symbol.to_string(),
            trade_no: trade_no.to_string(),
            trade_type,
            uid: uid.to_string(),
            audit: audit,
        };
        let res = ApiWithdrawDomain::withdraw(&req).await;
        match res {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::error!("withdrawal_order failed with {:?}", e);
                Err(e.into())
            }
        }
    }

    pub async fn sign_withdrawal_order(&self, order_id: &str) -> Result<(), ServiceError> {
        let backend_api = self.ctx.get_global_backend_api();

        let req = AuditResultReportReq::new(order_id.to_string(), true, "OK");
        backend_api.report_audit_result(&req).await?;

        ApiWithdrawDomain::sign_withdrawal_order(order_id).await
    }

    pub async fn reject_withdrawal_order(&self, order_id: &str) -> Result<(), ServiceError> {
        let backend_api = self.ctx.get_global_backend_api();

        let req = AuditResultReportReq::new(order_id.to_string(), false, "user rejected");
        backend_api.report_audit_result(&req).await?;

        ApiWithdrawDomain::reject_withdrawal_order(order_id).await
    }
}
