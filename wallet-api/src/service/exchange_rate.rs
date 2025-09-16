use wallet_database::{
    entities::exchange_rate::ExchangeRateEntity,
    repositories::{ResourcesRepo, TransactionTrait as _, exchange_rate::ExchangeRateRepoTrait},
};

use crate::domain::coin::CoinDomain;

pub struct ExchangeRateService {
    pub repo: ResourcesRepo,
    pub coin_domain: CoinDomain,
}

impl ExchangeRateService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo, coin_domain: CoinDomain::new() }
    }

    pub async fn upsert(
        self,
        target_currency: &str,
        name: &str,
        price: f64,
    ) -> Result<Vec<ExchangeRateEntity>, crate::error::service::ServiceError> {
        let mut tx = self.repo;
        let res = tx.upsert(target_currency, name, price).await?;
        Ok(res)
    }

    pub async fn detail(
        self,
        target_currency: Option<String>,
    ) -> Result<Option<ExchangeRateEntity>, crate::error::service::ServiceError> {
        let mut tx = self.repo;
        let res = tx.detail(target_currency).await?;
        Ok(res)
    }

    pub async fn init(
        self,
        rates: wallet_transport_backend::response_vo::coin::TokenRates,
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut tx = self.repo;
        tx.begin_transaction().await?;

        for rate in rates.list.into_iter() {
            tx.upsert(&rate.target_currency, &rate.name, rate.rate).await?;
        }
        tx.commit_transaction().await?;
        Ok(())
    }
}
