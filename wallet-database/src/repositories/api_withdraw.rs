use crate::{
    DbPool,
    dao::{Dao, api_withdraw::ApiWithdrawDao},
    entities::api_withdraw::ApiWithdrawEntity,
};

pub struct ApiWithdrawRepo;

impl ApiWithdrawRepo {
    pub async fn upsert(
        pool: &DbPool,
        uid: &str,
        name: &str,
        from_addr: &str,
        to_addr: &str,
        value: &str,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::upsert(
            pool.as_ref(),
            ApiWithdrawEntity {
                id: 0,
                name: name.to_string(),
                uid: uid.to_string(),
                from_addr: from_addr.to_string(),
                to_addr: to_addr.to_string(),
                value: value.to_string(),
                decimals: 0,
                token_addr: "".to_string(),
                symbol: "".to_string(),
                trade_no: "".to_string(),
                trade_type: "".to_string(),
                status: 0,
                created_at: Default::default(),
                updated_at: None,
            },
        )
        .await
    }

    pub async fn update_status(
        pool: &DbPool,
        trade_no: &str,
        merchant_id: &str,
        api_wallet_type: ApiWithdrawEntity,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        // Ok(ApiWithdrawDao::update_merchain_id(
        //     self.db_pool.as_ref(),
        //     address,
        //     merchant_id,
        //     api_wallet_type,
        // )
        // .await?)
        Ok(vec![])
    }
}
