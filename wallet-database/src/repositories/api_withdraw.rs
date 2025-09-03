use crate::{
    DbPool,
    dao::api_withdraw::ApiWithdrawDao,
    entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
};

pub struct ApiWithdrawRepo;

impl ApiWithdrawRepo {
    pub async fn list_api_withdraw(pool: &DbPool) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::all_api_withdraw(pool.as_ref()).await
    }

    pub async fn page_api_withdraw(
        pool: &DbPool,
        page: i64,
        page_size: i64,
    ) -> Result<(i64, Vec<ApiWithdrawEntity>), crate::Error> {
        ApiWithdrawDao::page_api_withdraw(pool.as_ref(), page, page_size).await
    }

    pub async fn page_api_withdraw_with_status(
        pool: &DbPool,
        page: i64,
        page_size: i64,
        vec_status: &[ApiWithdrawStatus],
    ) -> Result<(i64, Vec<ApiWithdrawEntity>), crate::Error> {
        ApiWithdrawDao::page_api_withdraw_with_status(pool.as_ref(), page, page_size, vec_status)
            .await
    }

    pub async fn upsert_api_withdraw(
        pool: &DbPool,
        uid: &str,
        name: &str,
        from_addr: &str,
        to_addr: &str,
        value: &str,
        chain_code: &str,
        token_addr: Option<String>,
        symbol: &str,
        trade_no: &str,
        trade_type: u8,
    ) -> Result<(), crate::Error> {
        let withdraw_req = ApiWithdrawEntity {
            id: 0,
            name: name.to_string(),
            uid: uid.to_string(),
            from_addr: from_addr.to_string(),
            to_addr: to_addr.to_string(),
            value: value.to_string(),
            chain_code: chain_code.to_string(),
            token_addr,
            symbol: symbol.to_string(),
            trade_no: trade_no.to_string(),
            trade_type,
            status: ApiWithdrawStatus::Init,
            tx_hash: "".to_string(),
            resource_consume: "".to_string(),
            transaction_fee: "".to_string(),
            transaction_time: None,
            block_height: "".to_string(),
            notes: "".to_string(),
            post_tx_count: 0,
            created_at: Default::default(),
            updated_at: None,
        };
        ApiWithdrawDao::add(pool.as_ref(), withdraw_req).await
    }

    pub async fn update_api_withdraw_tx_status(
        pool: &DbPool,
        trade_no: &str,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_tx_status(
            pool.as_ref(),
            trade_no,
            tx_hash,
            resource_consume,
            transaction_fee,
            status,
        )
        .await
    }

    pub async fn update_api_withdraw_status(
        pool: &DbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_status(pool.as_ref(), trade_no, status).await
    }

    pub async fn update_api_withdraw_next_status(
        pool: &DbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
        next_status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_next_status(pool.as_ref(), trade_no, status, next_status).await
    }
}
