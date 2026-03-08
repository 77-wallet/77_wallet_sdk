use crate::{
    CoreDbPool,
    dao::assets::CreateAssetsVo,
    entities::assets::{AssetsEntity, AssetsEntityWithAddressType, AssetsId},
};

impl super::RepoCtx {
    pub async fn upsert_assets(&mut self, assets: CreateAssetsVo) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AssetsEntity::upsert_assets, assets)
    }

    pub async fn update_is_multisig(&mut self, id: &AssetsId) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AssetsEntity::update_is_multisig, id)
    }

    pub async fn update_balance(
        &mut self,
        id: &AssetsId,
        balance: &str,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AssetsEntity::update_balance, id, balance)
    }

    pub async fn update_status(
        &mut self,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        status: u8,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::update_status,
            chain_code,
            symbol,
            token_address,
            status
        )
    }

    pub async fn unactived_list(&mut self) -> Result<Vec<AssetsEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AssetsEntity::unactived_list,)
    }

    pub async fn assets_by_id(
        &mut self,
        id: &AssetsId,
    ) -> Result<Option<AssetsEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AssetsEntity::assets_by_id, id)
    }

    pub async fn list_by_chain_token_map_batch(
        &mut self,
        pool: &CoreDbPool,
        chain_list: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        AssetsEntity::list_by_chain_token_map_batch(pool.as_ref(), chain_list).await
    }

    pub async fn get_chain_assets_by_address_chain_code_symbol(
        &mut self,
        address: Vec<String>,
        chain_code: Option<String>,
        symbol: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::get_chain_assets_by_address_chain_code_symbol,
            address,
            chain_code,
            symbol,
            is_multisig
        )
    }

    pub async fn get_assets_by_address_tx(
        &mut self,
        address: Vec<String>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AssetsEntityWithAddressType>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::get_assets_by_address,
            address,
            None,
            None,
            None,
            is_multisig
        )
    }

    pub async fn get_coin_assets_in_address_all_status(
        &mut self,
        addresses: Vec<String>,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::get_coin_assets_in_address,
            addresses,
            None
        )
    }

    pub async fn get_coin_assets_in_address(
        &mut self,
        addresses: Vec<String>,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::get_coin_assets_in_address,
            addresses,
            Some(1)
        )
    }

    pub async fn lists(
        &mut self,
        addr: Vec<String>,
        chain_code: Option<String>,
        keyword: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::all_assets,
            addr,
            chain_code,
            keyword,
            is_multisig
        )
    }

    pub async fn delete_multi_assets(
        &mut self,
        assets_ids: Vec<AssetsId>,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AssetsEntity::delete_multi_assets, assets_ids)
    }
}

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
