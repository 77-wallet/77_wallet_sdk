use crate::{
    CollectDbPool,
    dao::api_collect::ApiCollectDao,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus}, 
};

pub struct ApiCollectRepo;

impl ApiCollectRepo {
    pub async fn list_api_collect(pool: &CollectDbPool) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::all_api_collect(pool.as_ref()).await
    }

    pub async fn page_api_collect(
        pool: &CollectDbPool,
        _page: i64,
        _page_size: i64,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::all_api_collect(pool.as_ref()).await
    }

    pub async fn page_api_collect_with_status(
        pool: &CollectDbPool,
        page: i64,
        page_size: i64,
        vec_status: &[ApiCollectStatus],
    ) -> Result<(i64, Vec<ApiCollectEntity>), crate::Error> {
        ApiCollectDao::page_api_collect_with_status(pool.as_ref(), page, page_size, vec_status)
            .await
    }

    pub async fn get_api_collect_by_trade_no(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<ApiCollectEntity, crate::Error> {
        ApiCollectDao::get_api_collect_by_trade_no(pool.as_ref(), trade_no).await
    }

    pub async fn get_api_collect_by_trade_no_status(
        pool: &CollectDbPool,
        trade_no: &str,
        vec_status: &[ApiCollectStatus],
    ) -> Result<ApiCollectEntity, crate::Error> {
        ApiCollectDao::get_api_collect_by_trade_no_status(pool.as_ref(), trade_no, vec_status).await
    }

    pub async fn upsert_api_collect(
        pool: &CollectDbPool,
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
        risk_addr: u8,
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
            risk_addr,
            status,
            nonce: 0,
            tx_hash: "".to_string(),
            raw_tx: "".to_string(),
            resource_consume: "".to_string(),
            transaction_fee: "".to_string(),
            transaction_time: None,
            block_height: "".to_string(),
            notes: "".to_string(),
            post_tx_count: 0,
            post_confirm_tx_count: 0,
            err_code: 0,
            err_msg: "".to_string(),
            created_at: Default::default(),
            updated_at: None,
            tx_ack_sent_at: None,
            tx_res_ack_sent_at: None,
        };
        ApiCollectDao::add(pool.as_ref(), collect_req).await
    }

    pub async fn update_api_collect_to_addr(
        pool: &CollectDbPool,
        trade_no: &str,
        to_addr: &str,
    ) -> Result<(), crate::Error> {
        ApiCollectDao::update_to_addr(pool.as_ref(), trade_no, to_addr).await
    }

    pub async fn update_api_collect_tx_status_nonce(
        pool: &CollectDbPool,
        from_addr: &str,
        chain_code: &str,
        trade_no: &str,
        nonce: i64,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiCollectStatus,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_tx_status_nonce(
            &pool.into_inner(),
            from_addr,
            chain_code,
            trade_no,
            nonce,
            tx_hash,
            resource_consume,
            transaction_fee,
            status,
        )
        .await
    }
    pub async fn update_api_collect_tx_status(
        pool: &CollectDbPool,
        trade_no: &str,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiCollectStatus,
    ) -> Result<u64, crate::Error> {
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

    pub async fn update_api_collect_status_and_err(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiCollectStatus,
        err_code: u32,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_status_and_err(pool.as_ref(), trade_no, status, err_code, err_msg)
            .await
    }

    pub async fn update_api_collect_next_status(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiCollectStatus,
        next_status: ApiCollectStatus,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_next_status(pool.as_ref(), trade_no, status, next_status).await
    }

    pub async fn update_api_collect_next_status_and_err(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiCollectStatus,
        next_status: ApiCollectStatus,
        err_code: u32,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_next_status_and_err(
            pool.as_ref(),
            trade_no,
            status,
            next_status,
            err_code,
            err_msg,
        )
        .await
    }

    pub async fn update_api_collect_post_tx_count(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiCollectStatus,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_post_tx_count(pool.as_ref(), trade_no, status).await
    }

    pub async fn update_api_collect_post_confirm_tx_count(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiCollectStatus,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_post_confirm_tx_count(pool.as_ref(), trade_no, status).await
    }

    pub async fn update_after_build(
        pool: &CollectDbPool,
        trade_no: &str,
        tx_hash: &str,
        raw_tx: &str,
        transaction_fee: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_after_build(pool.as_ref(), trade_no, tx_hash, raw_tx, transaction_fee)
            .await
    }

    pub async fn set_tx_ack_sent(pool: &CollectDbPool, trade_no: &str) -> Result<(), crate::Error> {
        ApiCollectDao::set_tx_ack_sent(pool.as_ref(), trade_no).await
    }

    pub async fn set_tx_res_ack_sent(pool: &CollectDbPool, trade_no: &str) -> Result<(), crate::Error> {
        ApiCollectDao::set_tx_res_ack_sent(pool.as_ref(), trade_no).await
    }

    pub async fn get_ack_times(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<
        (
            Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
            Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        ),
        crate::Error,
    > {
        ApiCollectDao::get_ack_times(pool.as_ref(), trade_no).await
    }
}