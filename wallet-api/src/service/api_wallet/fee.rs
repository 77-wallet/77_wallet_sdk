use wallet_database::{
    entities::api_fee::ApiFeeEntity,
    repositories::{ResourcesRepo, api_fee::ApiFeeRepo},
};

use crate::{domain::api_wallet::fee::ApiFeeDomain, request::api_wallet::trans::ApiTransferFeeReq};

pub struct TransferFeeService {
    pub repo: ResourcesRepo,
}

impl TransferFeeService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo }
    }

    pub async fn get_transfer_fee_order_list(
        &self,
    ) -> Result<Vec<ApiFeeEntity>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiFeeRepo::list_api_fee(&pool).await.map_err(|e| e.into())
    }

    pub async fn transfer_fee_order(
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
        let req = ApiTransferFeeReq {
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
        let res = ApiFeeDomain::transfer_fee(&req).await;
        match res {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::error!("withdrawal_order failed with {:?}", e);
                Err(e.into())
            }
        }
    }
}
