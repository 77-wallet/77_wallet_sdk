use crate::{
    DbPool,
    entities::chain::{ChainCreateVo, ChainEntity, ChainWithNode},
};

pub struct ChainRepo;

impl ChainRepo {
    pub async fn detail(
        pool: &DbPool,
        chain_code: &str,
    ) -> Result<Option<ChainEntity>, crate::Error> {
        Ok(ChainEntity::detail(pool.as_ref(), chain_code).await?)
    }

    pub async fn add(pool: &DbPool, input: ChainCreateVo) -> Result<ChainEntity, crate::Error> {
        Ok(ChainEntity::upsert(pool.as_ref(), input).await?)
    }

    pub async fn set_chain_node(
        pool: &DbPool,
        chain_code: &str,
        node_id: &str,
    ) -> Result<Vec<ChainEntity>, crate::Error> {
        Ok(ChainEntity::set_chain_node(pool.as_ref(), chain_code, node_id).await?)
    }

    pub async fn get_chain_list(pool: &DbPool) -> Result<Vec<ChainEntity>, crate::Error> {
        Ok(ChainEntity::list(pool.as_ref(), Some(1)).await?)
    }

    pub async fn detail_with_node(
        pool: &DbPool,
        chain_code: &str,
    ) -> Result<Option<ChainWithNode>, crate::Error> {
        Ok(ChainEntity::chain_node_info(pool.as_ref(), chain_code).await?)
    }

    pub async fn toggle_chains_status(
        pool: &DbPool,
        chain_codes: &[String],
    ) -> Result<Vec<ChainEntity>, crate::Error> {
        Ok(ChainEntity::toggle_chains_status(pool.as_ref(), chain_codes).await?)
    }

    pub async fn upsert_multi_chain(
        pool: &DbPool,
        input: Vec<ChainCreateVo>,
    ) -> Result<(), crate::Error> {
        ChainEntity::upsert_multi_chain(pool.as_ref(), input).await
    }
}

#[async_trait::async_trait]
pub trait ChainRepoTrait: super::TransactionTrait {
    async fn set_chain_node_id_empty(
        &mut self,
        node_id: &str,
    ) -> Result<Vec<ChainEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, ChainEntity::set_chain_node_id_empty, node_id)
    }

    async fn get_chain_list_v2(&mut self) -> Result<Vec<ChainEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, ChainEntity::list_v2, Some(1))
    }

    async fn get_chain_list_all_status(&mut self) -> Result<Vec<ChainEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, ChainEntity::list, None)
    }

    // async fn upsert_multi_chain(&mut self, input: Vec<ChainCreateVo>) -> Result<(), crate::Error> {
    //     let executor = self.get_conn_or_tx()?;
    //     crate::execute_with_executor!(executor, ChainEntity::upsert_multi_chain, input)
    // }

    async fn get_chain_node_list(&mut self) -> Result<Vec<ChainWithNode>, crate::Error> {
        let executor = self.get_conn_or_tx()?;

        crate::execute_with_executor!(executor, ChainEntity::list_with_node_info,)
    }

    async fn detail(&mut self, chain_code: &str) -> Result<Option<ChainEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, ChainEntity::detail, chain_code)
    }

    async fn detail_by_id(&mut self, node_id: &str) -> Result<Option<ChainEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, ChainEntity::detail_by_id, node_id)
    }

    async fn detail_with_main_symbol(
        &mut self,
        main_symbol: &str,
    ) -> Result<Option<ChainEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, ChainEntity::detail_with_main_symbol, main_symbol)
    }

    async fn chain_node_info_left_join(
        &mut self,
        chain_code: &str,
    ) -> Result<Option<ChainWithNode>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, ChainEntity::chain_node_info_left_join, chain_code)
    }
}
