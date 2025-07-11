use crate::{
    dao::assets::CreateAssetsVo,
    entities::assets::{AssetsEntity, AssetsEntityWithAddressType, AssetsId, WalletType},
};

// pub struct ChainRepo {
//     // pub repo: ResourcesRepo,
// }

// impl ChainRepo {
//     pub fn new(db_pool: crate::DbPool) -> Self {
//         Self {
//             // repo: ResourcesRepo::new(db_pool),
//         }
//     }
// }

// impl ChainRepoTrait for ChainRepo {}

#[async_trait::async_trait]
pub trait AssetsRepoTrait: super::TransactionTrait {
    async fn upsert_assets(
        &mut self,
        assets: CreateAssetsVo,
        wallet_type: WalletType,
    ) -> Result<(), crate::Error> {
        // let wallet_type = GLOBAL_WALLET_TYPE.get_or_error().await?;
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AssetsEntity::upsert_assets, assets, wallet_type)
    }

    async fn update_is_multisig(
        &mut self,
        id: &AssetsId,
        wallet_type: WalletType,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AssetsEntity::update_is_multisig, id, wallet_type)
    }

    async fn update_balance(
        &mut self,
        id: &AssetsId,
        balance: &str,
        wallet_type: Option<WalletType>,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::update_balance,
            id,
            balance,
            wallet_type
        )
    }

    async fn update_status(
        &mut self,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        status: u8,
        wallet_type: WalletType,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::update_status,
            chain_code,
            symbol,
            token_address,
            wallet_type,
            status
        )
    }

    async fn unactived_list(
        &mut self,
        wallet_type: WalletType,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AssetsEntity::unactived_list, wallet_type)
    }

    async fn assets_by_id(
        &mut self,
        id: &AssetsId,
        wallet_type: WalletType,
    ) -> Result<Option<AssetsEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AssetsEntity::assets_by_id, id, wallet_type)
    }

    async fn get_chain_assets_by_address_chain_code_symbol(
        &mut self,
        address: Vec<String>,
        chain_code: Option<String>,
        symbol: Option<&str>,
        is_multisig: Option<bool>,
        wallet_type: WalletType,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::get_chain_assets_by_address_chain_code_symbol,
            address,
            chain_code,
            symbol,
            wallet_type,
            is_multisig
        )
    }

    async fn get_assets_by_address(
        &mut self,
        address: Vec<String>,
        chain_code: Option<String>,
        symbol: Option<&str>,
        is_multisig: Option<bool>,
        wallet_type: WalletType,
    ) -> Result<Vec<AssetsEntityWithAddressType>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::get_assets_by_address,
            address,
            chain_code,
            symbol,
            is_multisig,
            wallet_type
        )
    }

    async fn get_coin_assets_in_address_all_status(
        &mut self,
        addresses: Vec<String>,
        wallet_type: WalletType,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::get_coin_assets_in_address,
            addresses,
            None,
            wallet_type
        )
    }

    async fn get_coin_assets_in_address(
        &mut self,
        addresses: Vec<String>,
        wallet_type: WalletType,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::get_coin_assets_in_address,
            addresses,
            Some(1),
            wallet_type
        )
    }

    async fn lists(
        &mut self,
        addr: Vec<String>,
        chain_code: Option<String>,
        keyword: Option<&str>,
        is_multisig: Option<bool>,
        wallet_type: Option<WalletType>,
    ) -> Result<Vec<AssetsEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::all_assets,
            addr,
            chain_code,
            keyword,
            wallet_type,
            is_multisig
        )
    }

    async fn delete_multi_assets(
        &mut self,
        assets_ids: Vec<AssetsId>,
        wallet_type: WalletType,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            AssetsEntity::delete_multi_assets,
            assets_ids,
            wallet_type
        )
    }
}
