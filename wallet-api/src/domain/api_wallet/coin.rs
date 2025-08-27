use wallet_database::entities::coin::CoinEntity;

pub(crate) struct ApiCoinDomain;

impl ApiCoinDomain {
    pub async fn main_coin(chain_code: &str) -> Result<CoinEntity, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let coin = CoinEntity::main_coin(chain_code, pool.as_ref()).await?.ok_or(
            crate::BusinessError::Coin(crate::error::business::coin::CoinError::NotFound(format!(
                "chian = {}",
                chain_code
            ))),
        )?;
        Ok(coin)
    }
}