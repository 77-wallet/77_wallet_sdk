use crate::{
    DbPool,
    dao::api_withdraw::ApiWithdrawDao,
    entities::{
        api_trade_type::ApiWithdrawTradeType,
        api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
    },
    pagination::Pagination,
};

pub struct ApiWithdrawRepo;

impl ApiWithdrawRepo {
    pub async fn list_api_withdraw(
        pool: &DbPool,
        uid: &str,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::all_api_withdraw(pool.as_ref(), uid).await
    }

    pub async fn list_api_withdraw_with_status(
        pool: &DbPool,
        status: Vec<ApiWithdrawStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::list_api_withdraw_with_status(pool.as_ref(), status, page, page_size).await
    }

    pub async fn page_api_withdraw(
        pool: &DbPool,
        uid: &str,
        status: Vec<ApiWithdrawStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::page_api_withdraw(pool.as_ref(), uid, status, page, page_size).await
    }

    pub async fn page_api_withdraw_with_init_status(
        pool: &DbPool,
        uid: &str,
        init_status: ApiWithdrawStatus,
        status: Vec<ApiWithdrawStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::page_api_withdraw_with_init_status(
            pool.as_ref(),
            uid,
            init_status,
            status,
            page,
            page_size,
        )
        .await
    }

    pub async fn get_api_withdraw_by_id(
        pool: &DbPool,
        id: &str,
    ) -> Result<ApiWithdrawEntity, crate::Error> {
        ApiWithdrawDao::get_api_withdraw_by_id(pool.as_ref(), id).await
    }

    pub async fn get_api_withdraw_by_trade_no(
        pool: &DbPool,
        trade_no: &str,
    ) -> Result<ApiWithdrawEntity, crate::Error> {
        ApiWithdrawDao::get_api_withdraw_by_trade_no(pool.as_ref(), trade_no).await
    }

    pub async fn get_api_withdraw_by_trade_no_status(
        pool: &DbPool,
        trade_no: &str,
        vec_status: &[ApiWithdrawStatus],
    ) -> Result<ApiWithdrawEntity, crate::Error> {
        ApiWithdrawDao::get_api_withdraw_by_trade_no_status(pool.as_ref(), trade_no, vec_status)
            .await
    }

    pub async fn get_by_hash_and_owner(
        pool: &DbPool,
        owner: &str,
        tx_hash: &str,
    ) -> Result<ApiWithdrawEntity, crate::Error> {
        ApiWithdrawDao::get_by_hash_and_owner(pool.as_ref(), owner, tx_hash).await
    }

    pub async fn lists_by_hashs(
        pool: &DbPool,
        owner: &str,
        hashs: Vec<String>,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::lists_by_hashs(pool.as_ref(), owner, hashs).await
    }

    pub async fn recent_bill(
        pool: &DbPool,
        token: &str,
        from_addr: &str,
        chain_code: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error> {
        let lists = ApiWithdrawDao::recent_bill(
            pool.as_ref(),
            token,
            from_addr,
            chain_code,
            page,
            page_size,
        )
        .await?;
        Ok(lists)
    }

    pub async fn bill_lists(
        pool: &DbPool,
        uid: &str,
        addr: &[String],
        chain_code: Option<&str>,
        symbol: Option<&str>,
        is_multisig: Option<i64>,
        min_value: Option<f64>,
        start: Option<i64>,
        end: Option<i64>,
        transfer_type: Vec<i32>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error> {
        let lists = ApiWithdrawDao::bill_lists(
            pool.as_ref(),
            uid,
            addr,
            chain_code,
            symbol,
            is_multisig,
            min_value,
            start,
            end,
            transfer_type,
            page,
            page_size,
        )
        .await?;
        Ok(lists)
    }

    pub async fn upsert_api_withdraw(
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
        trade_type: ApiWithdrawTradeType,
        tx_hash: &str,
        status: ApiWithdrawStatus,
        transaction_fee: &str,
        transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        block_height: &str,
    ) -> Result<(), crate::Error> {
        let withdraw_req = ApiWithdrawEntity {
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
            init_status: status,
            status,
            tx_hash: tx_hash.to_string(),
            resource_consume: "".to_string(),
            transaction_fee: transaction_fee.to_string(),
            transaction_time,
            block_height: block_height.to_string(),
            notes: "".to_string(),
            post_tx_count: 0,
            post_confirm_tx_count: 0,
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
        transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        block_height: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_tx_status(
            pool.as_ref(),
            trade_no,
            tx_hash,
            resource_consume,
            transaction_fee,
            transaction_time,
            block_height,
            status,
        )
        .await
    }

    pub async fn update_api_withdraw_status(
        pool: &DbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
        notes: &str,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_status(pool.as_ref(), trade_no, status, notes).await
    }

    pub async fn update_api_withdraw_next_status(
        pool: &DbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
        next_status: ApiWithdrawStatus,
        notes: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_next_status(pool.as_ref(), trade_no, status, next_status, notes)
            .await
    }

    pub async fn update_api_fee_post_tx_count(
        pool: &DbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_post_tx_count(pool.as_ref(), trade_no, status).await
    }

    pub async fn update_api_withdraw_post_confirm_tx_count(
        pool: &DbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_post_confirm_tx_count(pool.as_ref(), trade_no, status).await
    }
}
