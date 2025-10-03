use crate::{
    DbPool,
    dao::api_assets::ApiAssetsDao,
    entities::{
        api_assets::{ApiAssetsEntity, ApiCreateAssetsVo},
        assets::AssetsIdVo,
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
        address: &str,
        chain_code: &str,
        token_address: Option<String>,
        balance: &str,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::update_balance(pool.as_ref(), address, chain_code, token_address, balance)
            .await
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
        id: &AssetsIdVo<'_>,
    ) -> Result<Option<ApiAssetsEntity>, crate::Error> {
        Ok(ApiAssetsDao::assets_by_id(pool.as_ref(), id).await?)
    }

    pub async fn list(pool: &DbPool) -> Result<Vec<ApiAssetsEntity>, crate::Error> {
        Ok(ApiAssetsDao::list(pool.as_ref()).await?)
    }

    pub async fn get_chain_assets_by_address_chain_code_symbol(
        pool: &DbPool,
        address: Vec<String>,
        chain_code: Option<String>,
        symbol: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<ApiAssetsEntity>, crate::Error> {
        ApiAssetsDao::get_chain_assets_by_address_chain_code_symbol(
            pool.as_ref(),
            address,
            chain_code,
            symbol,
            is_multisig,
        )
        .await
    }
}
