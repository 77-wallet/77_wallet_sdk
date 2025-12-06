use crate::{
    DbPool,
    entities::exchange_rate::{ExchangeRateEntity, QueryReq},
};

pub struct ExchangeRateRepo;

impl ExchangeRateRepo {
    pub async fn upsert(
        pool: &DbPool,
        target_currency: &str,
        name: &str,
        rate: f64,
    ) -> Result<Vec<ExchangeRateEntity>, crate::Error> {
        let executor = pool.as_ref();
        ExchangeRateEntity::upsert(executor, target_currency, name, rate).await
    }

    pub async fn list(pool: &DbPool) -> Result<Vec<ExchangeRateEntity>, crate::Error> {
        ExchangeRateEntity::list(pool.as_ref()).await
    }

    // get exchange rate by target currency
    pub async fn exchange_rate(
        target: &str,
        pool: &DbPool,
    ) -> Result<ExchangeRateEntity, crate::Error> {
        let query_req = QueryReq { target_currency: Some(target.to_string()) };
        ExchangeRateEntity::detail(pool.as_ref(), &query_req)
            .await?
            .ok_or(crate::Error::NotFound(format!("exchange rate not found currency: {}", target)))
    }

    pub async fn detail(
        pool: &DbPool,
        target_currency: Option<String>,
    ) -> Result<Option<ExchangeRateEntity>, crate::Error> {
        let req = QueryReq { target_currency };
        ExchangeRateEntity::detail(pool.as_ref(), &req).await
    }
}
