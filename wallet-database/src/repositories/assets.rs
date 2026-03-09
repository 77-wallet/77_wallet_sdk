use crate::{
    CoreDbPool,
    entities::assets::{AssetsEntity, AssetsEntityWithAddressType},
};

pub struct AssetsRepo;

impl AssetsRepo {
    pub async fn get_assets_by_address(
        pool: &CoreDbPool,
        address: Vec<String>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AssetsEntityWithAddressType>, crate::Error> {
        AssetsEntity::get_assets_by_address(pool.as_ref(), address, None, None, None, is_multisig)
            .await
    }

    pub async fn get_by_addr_token(
        pool: &CoreDbPool,
        chain_code: &str,
        token_address: &str,
        address: &str,
    ) -> Result<AssetsEntity, crate::Error> {
        AssetsEntity::get_by_addr_token(pool.as_ref(), chain_code, token_address, address)
            .await?
            .ok_or(crate::Error::NotFound(format!(
                "asset not found chain_code {}, token_address {}, address {}",
                chain_code, token_address, address
            )))
    }

    // option 类型
    pub async fn get_by_addr_token_opt(
        pool: &CoreDbPool,
        chain_code: &str,
        token_address: &str,
        address: &str,
    ) -> Result<Option<AssetsEntity>, crate::Error> {
        AssetsEntity::get_by_addr_token(pool.as_ref(), chain_code, token_address, address).await
    }

    // repair
    pub async fn all_error_wsol(pool: &CoreDbPool) -> Result<Vec<AssetsEntity>, crate::Error> {
        AssetsEntity::error_wsol_assets(pool.as_ref()).await
    }

    pub async fn repair_wsol_error(pool: &CoreDbPool) -> Result<(), crate::Error> {
        AssetsEntity::delete_error_wsol_assets(pool.as_ref()).await
    }
}
