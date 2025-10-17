use crate::{
    DbPool,
    entities::exchange_rate::{ExchangeRateEntity, QueryReq},
};

#[async_trait::async_trait]
pub trait ExchangeRateRepoTrait: super::TransactionTrait {
    async fn upsert(
        &mut self,
        target_currency: &str,
        name: &str,
        rate: f64,
    ) -> Result<Vec<ExchangeRateEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            ExchangeRateEntity::upsert,
            target_currency,
            name,
            rate
        )
    }

    async fn detail(
        &mut self,
        target_currency: Option<String>,
    ) -> Result<Option<ExchangeRateEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        let req = crate::entities::exchange_rate::QueryReq { target_currency };
        crate::execute_with_executor!(executor, ExchangeRateEntity::detail, &req)
    }
}

pub struct ExchangeRateRepo;

impl ExchangeRateRepo {
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
}
