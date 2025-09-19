use crate::{
    DbPool,
    dao::api_assets::ApiAssetsDao,
    entities::{
        api_assets::{ApiAssetsEntity, ApiCreateAssetsVo},
        assets::AssetsId,
    },
};

pub struct ApiAssetsRepo;

impl ApiAssetsRepo {
    pub async fn upsert_assets(
        pool: &DbPool,
        assets: ApiCreateAssetsVo,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::upsert_assets(pool.as_ref(), assets).await
    }

    pub async fn update_balance(
        pool: &DbPool,
        id: &AssetsId,
        balance: &str,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::update_balance(pool.as_ref(), id, balance).await
    }

    pub async fn update_status(
        pool: &DbPool,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        status: u8,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::update_status(pool.as_ref(), chain_code, symbol, token_address, status).await
    }

    pub async fn find_by_id(
        pool: &DbPool,
        id: &AssetsId,
    ) -> Result<Option<ApiAssetsEntity>, crate::Error> {
        Ok(ApiAssetsDao::assets_by_id(pool.as_ref(), id).await?)
    }

    pub async fn list(pool: &DbPool) -> Result<Vec<ApiAssetsEntity>, crate::Error> {
        Ok(ApiAssetsDao::list(pool.as_ref()).await?)
    }
}
