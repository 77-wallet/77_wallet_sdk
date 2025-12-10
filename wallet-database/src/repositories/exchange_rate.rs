use crate::{DbPool, entities::exchange_rate::ExchangeRateEntity};

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
        ExchangeRateEntity::detail(pool.as_ref(), target)
            .await?
            .ok_or(crate::Error::NotFound(format!("exchange rate not found currency: {}", target)))
    }
}
