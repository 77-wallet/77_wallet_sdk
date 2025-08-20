use crate::dao::api_assets::ApiAssetsDao;
use crate::entities::api_assets::ApiCreateAssetsVo;
use crate::entities::assets::{AssetsEntity, AssetsId};
use crate::DbPool;

pub struct ApiAssetsRepo;

impl ApiAssetsRepo {
    async fn upsert_assets(pool: &DbPool, assets: ApiCreateAssetsVo) -> Result<(), crate::Error> {
        ApiAssetsDao::upsert_assets(pool.as_ref(), assets).await
    }

    async fn update_is_multisig(pool: &DbPool, id: &AssetsId) -> Result<(), crate::Error> {
        // ApiAssetsDao::update_is_multisig
        Ok(())
    }

    async fn update_balance(
        pool: &DbPool,
        id: &AssetsId,
        balance: &str,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::update_balance(pool.as_ref(), id, balance).await
    }

    async fn update_status(
        pool: &DbPool,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        status: u8,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::update_status(pool.as_ref(), chain_code, symbol, token_address, status).await
    }

    async fn unactived_list(pool: &DbPool) -> Result<Vec<AssetsEntity>, crate::Error> {
        // ApiAssetsDao::unactived_list(pool.as_ref())
        Ok(vec![])
    }

    async fn assets_by_id(
        pool: &DbPool,
        id: &AssetsId,
    ) -> Result<Option<AssetsEntity>, crate::Error> {
        // ApiAssetsDao::assets_by_id(pool.as_ref(), id)
        Ok(None)
    }

    async fn get_chain_assets_by_address_chain_code_symbol(
        pool: &DbPool,
        address: Vec<String>,
        chain_code: Option<String>,
        symbol: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        AssetsEntity::get_chain_assets_by_address_chain_code_symbol(
            pool.as_ref(),
            address,
            chain_code,
            symbol,
            is_multisig,
        )
        .await
    }
}
