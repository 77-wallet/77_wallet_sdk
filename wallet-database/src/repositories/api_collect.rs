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

    pub async fn upsert_api_collect(
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
        let collect_req = ApiCollectEntity {
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
            status: ApiCollectStatus::Init,
            tx_hash: "".to_string(),
            resource_consume: "".to_string(),
            transaction_fee: "".to_string(),
            transaction_time: None,
            block_height: "".to_string(),
            notes: "".to_string(),
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
    ) -> Result<(), crate::Error> {
        ApiCollectDao::update_status(pool.as_ref(), trade_no, status).await
    }
}
