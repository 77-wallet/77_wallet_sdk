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

    pub async fn get_by_target_currency(
        pool: &DbPool,
        target_currency: &str,
    ) -> Result<Option<ExchangeRateEntity>, crate::Error> {
        ExchangeRateEntity::get_by_target_currency(pool.as_ref(), target_currency).await
    }

    pub async fn get_by_target_currency_or_default(
        pool: &DbPool,
        target_currency: &str,
    ) -> Result<ExchangeRateEntity, crate::Error> {
        Ok(ExchangeRateRepo::get_by_target_currency(pool, target_currency).await?.unwrap_or(
            ExchangeRateEntity {
                name: "USD".to_string(),
                rate: 1.0,
                target_currency: "USD".to_string(),
                created_at: Default::default(),
                updated_at: Default::default(),
            },
        ))
    }


}
