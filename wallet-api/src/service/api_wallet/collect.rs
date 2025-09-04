use crate::{
    domain::api_wallet::trans::collect::ApiCollectDomain,
    request::api_wallet::trans::ApiWithdrawReq,
};
use wallet_database::{
    entities::api_collect::ApiCollectEntity, repositories::api_collect::ApiCollectRepo,
};

pub struct CollectService {}

impl CollectService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn get_collect_order_list(
        &self,
    ) -> Result<Vec<ApiCollectEntity>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiCollectRepo::list_api_collect(&pool).await.map_err(|e| e.into())
    }

    pub async fn collect_order(
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
        ApiCollectDomain::collect(&req).await?;
        Ok(())
    }
}
