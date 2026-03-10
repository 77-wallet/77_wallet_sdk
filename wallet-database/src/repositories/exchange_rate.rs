use crate::{
    CoreDbPool, entities::exchange_rate::ExchangeRateEntity,
};
use sqlx::{Sqlite, Transaction};

pub struct ExchangeRateRepo;

impl ExchangeRateRepo {
    pub async fn upsert(
        pool: CoreDbPool,
        target_currency: &str,
        name: &str,
        rate: f64,
    ) -> Result<Vec<ExchangeRateEntity>, crate::Error> {
        ExchangeRateEntity::upsert(pool.as_ref(), target_currency, name, rate).await
    }

    pub async fn upsert_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
        target_currency: &str,
        name: &str,
        rate: f64,
    ) -> Result<Vec<ExchangeRateEntity>, crate::Error> {
        ExchangeRateEntity::upsert(tx.as_mut(), target_currency, name, rate).await
    }

    pub async fn list(pool: CoreDbPool) -> Result<Vec<ExchangeRateEntity>, crate::Error> {
        ExchangeRateEntity::list(pool.as_ref()).await
    }

    pub async fn list_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
    ) -> Result<Vec<ExchangeRateEntity>, crate::Error> {
        ExchangeRateEntity::list(tx.as_mut()).await
    }

    // get exchange rate by target currency
    pub async fn exchange_rate(
        target: &str,
        pool: CoreDbPool,
    ) -> Result<ExchangeRateEntity, crate::Error> {
        ExchangeRateEntity::detail(pool.as_ref(), target)
            .await?
            .ok_or(crate::Error::NotFound(format!("exchange rate not found currency: {}", target)))
    }

    pub async fn exchange_rate_with_executor(
        target: &str,
        tx: &mut Transaction<'_, Sqlite>,
    ) -> Result<ExchangeRateEntity, crate::Error> {
        ExchangeRateEntity::detail(tx.as_mut(), target)
            .await?
            .ok_or(crate::Error::NotFound(format!("exchange rate not found currency: {}", target)))
    }

    pub async fn get_by_target_currency(
        pool: CoreDbPool,
        target_currency: &str,
    ) -> Result<Option<ExchangeRateEntity>, crate::Error> {
        ExchangeRateEntity::get_by_target_currency(pool.as_ref(), target_currency).await
    }

    pub async fn get_by_target_currency_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
        target_currency: &str,
    ) -> Result<Option<ExchangeRateEntity>, crate::Error> {
        ExchangeRateEntity::get_by_target_currency(tx.as_mut(), target_currency).await
    }

    pub async fn get_by_target_currency_or_default(
        pool: CoreDbPool,
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

    pub async fn get_by_target_currency_or_default_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
        target_currency: &str,
    ) -> Result<ExchangeRateEntity, crate::Error> {
        Ok(ExchangeRateRepo::get_by_target_currency_with_executor(tx, target_currency)
            .await?
            .unwrap_or(ExchangeRateEntity {
                name: "USD".to_string(),
                rate: 1.0,
                target_currency: "USD".to_string(),
                created_at: Default::default(),
                updated_at: Default::default(),
            }))
    }
}
