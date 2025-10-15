use crate::{
    DbPool,
    dao::api_collect::ApiCollectDao,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
};

pub struct ApiCollectRepo;

impl ApiCollectRepo {
    pub async fn list_api_collect(pool: &DbPool) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::all_api_collect(pool.as_ref()).await
    }

    pub async fn page_api_collect(
        pool: &DbPool,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::all_api_collect(pool.as_ref()).await
    }

    pub async fn page_api_collect_with_status(
        pool: &DbPool,
        page: i64,
        page_size: i64,
        vec_status: &[ApiCollectStatus],
    ) -> Result<(i64, Vec<ApiCollectEntity>), crate::Error> {
        ApiCollectDao::page_api_collect_with_status(pool.as_ref(), page, page_size, vec_status)
            .await
    }

    pub async fn get_api_collect_by_trade_no(
        pool: &DbPool,
        trade_no: &str,
    ) -> Result<ApiCollectEntity, crate::Error> {
        ApiCollectDao::get_api_collect_by_trade_no(pool.as_ref(), trade_no).await
    }

    pub async fn get_api_collect_by_trade_no_status(
        pool: &DbPool,
        trade_no: &str,
        vec_status: &[ApiCollectStatus],
    ) -> Result<ApiCollectEntity, crate::Error> {
        ApiCollectDao::get_api_collect_by_trade_no_status(pool.as_ref(), trade_no, vec_status).await
    }

    pub async fn upsert_api_collect(
        pool: &DbPool,
        uid: &str,
        name: &str,
        from_addr: &str,
        to_addr: &str,
        value: &str,
        validate: &str,
        chain_code: &str,
        token_addr: Option<String>,
        symbol: &str,
        trade_no: &str,
        trade_type: u8,
        status: ApiCollectStatus,
    ) -> Result<(), crate::Error> {
        let collect_req = ApiCollectEntity {
            id: 0,
            name: name.to_string(),
            uid: uid.to_string(),
            from_addr: from_addr.to_string(),
            to_addr: to_addr.to_string(),
            value: value.to_string(),
            validate: validate.to_string(),
            chain_code: chain_code.to_string(),
            token_addr,
            symbol: symbol.to_string(),
            trade_no: trade_no.to_string(),
            trade_type,
            status,
            tx_hash: "".to_string(),
            resource_consume: "".to_string(),
            transaction_fee: "".to_string(),
            transaction_time: None,
            block_height: "".to_string(),
            notes: "".to_string(),
            post_tx_count: 0,
            post_confirm_tx_count: 0,
            created_at: Default::default(),
            updated_at: None,
        };
        ApiCollectDao::add(pool.as_ref(), collect_req).await
    }

    pub async fn update_api_collect_tx_status(
        pool: &DbPool,
        trade_no: &str,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiCollectStatus,
    ) -> Result<(), crate::Error> {
        ApiCollectDao::update_tx_status(
            pool.as_ref(),
            trade_no,
            tx_hash,
            resource_consume,
            transaction_fee,
            status,
        )
        .await
    }

    pub async fn update_api_collect_status(
        pool: &DbPool,
        trade_no: &str,
        status: ApiCollectStatus,
        notes: &str,
    ) -> Result<(), crate::Error> {
        ApiCollectDao::update_status(pool.as_ref(), trade_no, status, notes).await
    }

    pub async fn update_api_collect_next_status(
        pool: &DbPool,
        trade_no: &str,
        status: ApiCollectStatus,
        next_status: ApiCollectStatus,
    ) -> Result<(), crate::Error> {
        ApiCollectDao::update_next_status(pool.as_ref(), trade_no, status, next_status).await
    }

    pub async fn update_api_collect_post_tx_count(
        pool: &DbPool,
        trade_no: &str,
        status: ApiCollectStatus,
    ) -> Result<(), crate::Error> {
        ApiCollectDao::update_post_tx_count(pool.as_ref(), trade_no, status).await
    }

    pub async fn update_api_collect_post_confirm_tx_count(
        pool: &DbPool,
        trade_no: &str,
        status: ApiCollectStatus,
    ) -> Result<(), crate::Error> {
        ApiCollectDao::update_post_confirm_tx_count(pool.as_ref(), trade_no, status).await
    }
}
