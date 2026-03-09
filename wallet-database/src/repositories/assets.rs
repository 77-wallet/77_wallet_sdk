use crate::{
    CoreDbPool,
    dao::assets::{AssetsDao, CreateAssetsVo},
    entities::assets::{AssetsEntity, AssetsEntityWithAddressType, AssetsId},
};
use sqlx::{Sqlite, Transaction};

pub struct AssetsRepo;

impl AssetsRepo {
    pub async fn get_coin_assets_in_address(
        pool: &CoreDbPool,
        address: Vec<String>,
        status: Option<u8>,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        AssetsDao::get_coin_assets_in_address(pool.as_ref(), address, status).await
    }

    pub async fn get_assets_by_address(
        pool: &CoreDbPool,
        address: Vec<String>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AssetsEntityWithAddressType>, crate::Error> {
        AssetsDao::get_assets_by_address(pool.as_ref(), address, None, None, None, is_multisig)
            .await
    }

    pub async fn assets_by_id(
        pool: &CoreDbPool,
        assets_id: &AssetsId,
    ) -> Result<Option<AssetsEntity>, crate::Error> {
        AssetsDao::assets_by_id(pool.as_ref(), assets_id).await
    }

    pub async fn upsert_assets(
        pool: &CoreDbPool,
        assets: CreateAssetsVo,
    ) -> Result<(), crate::Error> {
        AssetsDao::upsert_assets(pool.as_ref(), assets).await
    }

    pub async fn all_assets(
        pool: &CoreDbPool,
        addr: Vec<String>,
        chain_code: Option<String>,
        keyword: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        AssetsDao::all_assets(pool.as_ref(), addr, chain_code, keyword, is_multisig).await
    }

    pub async fn update_balance(
        pool: &CoreDbPool,
        id: &AssetsId,
        balance: &str,
    ) -> Result<(), crate::Error> {
        AssetsDao::update_balance(pool.as_ref(), id, balance).await
    }

    pub async fn delete_multi_assets(
        pool: &CoreDbPool,
        assets_ids: Vec<AssetsId>,
    ) -> Result<(), crate::Error> {
        AssetsDao::delete_multi_assets(pool.as_ref(), assets_ids).await
    }

    pub async fn update_balance_tx(
        tx: &mut Transaction<'_, Sqlite>,
        id: &AssetsId,
        balance: &str,
    ) -> Result<(), crate::Error> {
        AssetsDao::update_balance(tx.as_mut(), id, balance).await
    }

    pub async fn list_by_chain_token_map_batch(
        pool: &CoreDbPool,
        chain_list: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        AssetsDao::list_by_chain_token_map_batch(pool.as_ref(), chain_list).await
    }

    pub async fn get_chain_assets_by_address_chain_code_symbol(
        pool: &CoreDbPool,
        address: Vec<String>,
        chain_code: Option<String>,
        symbol: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        AssetsDao::get_chain_assets_by_address_chain_code_symbol(
            pool.as_ref(),
            address,
            chain_code,
            symbol,
            is_multisig,
        )
        .await
    }

    pub async fn get_by_addr_token(
        pool: &CoreDbPool,
        chain_code: &str,
        token_address: &str,
        address: &str,
    ) -> Result<AssetsEntity, crate::Error> {
        AssetsDao::get_by_addr_token(pool.as_ref(), chain_code, token_address, address)
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
        AssetsDao::get_by_addr_token(pool.as_ref(), chain_code, token_address, address).await
    }

    // repair
    pub async fn all_error_wsol(pool: &CoreDbPool) -> Result<Vec<AssetsEntity>, crate::Error> {
        AssetsDao::error_wsol_assets(pool.as_ref()).await
    }

    pub async fn repair_wsol_error(pool: &CoreDbPool) -> Result<(), crate::Error> {
        AssetsDao::delete_error_wsol_assets(pool.as_ref()).await
    }

    pub async fn update_tron_multisig_assets(
        pool: &CoreDbPool,
        address: &str,
        chain_code: &str,
        is_multisig: i8,
    ) -> Result<(), crate::Error> {
        AssetsDao::update_tron_multisig_assets(address, chain_code, is_multisig, pool.as_ref())
            .await
    }
}
