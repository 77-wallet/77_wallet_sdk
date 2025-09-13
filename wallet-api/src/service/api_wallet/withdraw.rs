use crate::{
    domain::api_wallet::trans::withdraw::ApiWithdrawDomain,
    request::api_wallet::trans::ApiWithdrawReq,
};
use wallet_database::{
    entities::api_withdraw::ApiWithdrawEntity, repositories::api_withdraw::ApiWithdrawRepo,
};
use wallet_transport_backend::request::api_wallet::audit::AuditResultReportReq;

pub struct WithdrawService {}

impl WithdrawService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn get_withdraw_order_list(
        &self,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiWithdrawRepo::list_api_withdraw(&pool).await.map_err(|e| e.into())
    }

    pub async fn withdrawal_order(
        &self,
        from: &str,
        to: &str,
        value: &str,
        chain_code: &str,
        token_address: Option<String>,
        symbol: &str,
        trade_no: &str,
        trade_type: u8,
        uid: &str,
    ) -> Result<(), crate::ServiceError> {
        let req = ApiWithdrawReq {
            from: from.to_string(),
            to: to.to_string(),
            value: value.to_string(),
            chain_code: chain_code.to_string(),
            token_address,
            symbol: symbol.to_string(),
            trade_no: trade_no.to_string(),
            trade_type,
            uid: uid.to_string(),
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

    pub async fn sign_withdrawal_order(
        &self,
        order_id: &str,
        status: i8,
    ) -> Result<(), crate::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let req = AuditResultReportReq::new();
        backend_api.report_audit_result(&req).await?;

        ApiWithdrawDomain::sign_withdrawal_order(order_id).await
    }

    pub async fn reject_withdrawal_order(
        &self,
        order_id: &str,
        status: i8,
    ) -> Result<(), crate::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let req = AuditResultReportReq::new();
        backend_api.report_audit_result(&req).await?;

        ApiWithdrawDomain::reject_withdrawal_order(order_id).await
    }
}
