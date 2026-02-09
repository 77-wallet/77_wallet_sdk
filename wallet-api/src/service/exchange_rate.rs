use wallet_database::{
    entities::exchange_rate::ExchangeRateEntity,
    repositories::{ResourcesRepo, exchange_rate::ExchangeRateRepo},
};

pub struct ExchangeRateService {
    pub repo: ResourcesRepo,
}

impl ExchangeRateService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo }
    }

    pub async fn upsert(
        self,
        target_currency: &str,
        name: &str,
        price: f64,
    ) -> Result<Vec<ExchangeRateEntity>, crate::error::service::ServiceError> {
        let core_pool = crate::context::get_context()?.core_pool()?;
        let res = ExchangeRateRepo::upsert(core_pool, target_currency, name, price).await?;
        Ok(res)
    }

    pub async fn detail(
        self,
        target_currency: &str,
    ) -> Result<ExchangeRateEntity, crate::error::service::ServiceError> {
        let core_pool = crate::context::get_context()?.core_pool()?;
        let res = ExchangeRateRepo::exchange_rate(target_currency, core_pool).await?;
        Ok(res)
    }

    pub async fn init(
        self,
        rates: wallet_transport_backend::response_vo::coin::TokenRates,
    ) -> Result<(), crate::error::service::ServiceError> {
        let core_pool = crate::context::get_context()?.core_pool()?;

        for rate in rates.list.into_iter() {
            ExchangeRateRepo::upsert(
                core_pool.clone(),
                &rate.target_currency,
                &rate.name,
                rate.rate,
            )
            .await?;
        }
        Ok(())
    }
}
