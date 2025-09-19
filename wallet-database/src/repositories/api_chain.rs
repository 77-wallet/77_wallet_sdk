use crate::{
    DbPool,
    dao::api_chain::ApiChainDao,
    entities::{
        api_chain::ApiChainEntity,
        chain::{ChainCreateVo, ChainEntity, ChainWithNode},
    },
};

pub struct ApiChainRepo;

impl ApiChainRepo {
    pub async fn get_chain_list(pool: &DbPool) -> Result<Vec<ChainEntity>, crate::Error> {
        Ok(ChainEntity::list(pool.as_ref(), Some(1)).await?)
    }

    pub async fn detail_with_node(
        pool: &DbPool,
        chain_code: &str,
    ) -> Result<Option<ChainWithNode>, crate::Error> {
        Ok(ChainEntity::chain_node_info(pool.as_ref(), chain_code).await?)
    }

    pub async fn detail(
        pool: &DbPool,
        chain_code: &str,
    ) -> Result<Option<ApiChainEntity>, crate::Error> {
        Ok(ApiChainDao::detail(pool.as_ref(), chain_code).await?)
    }

    pub async fn add(pool: &DbPool, input: ChainCreateVo) -> Result<(), crate::Error> {
        Ok(ApiChainDao::upsert(pool.as_ref(), input).await?)
    }

    pub async fn set_chain_node(
        pool: &DbPool,
        chain_code: &str,
        node_id: &str,
    ) -> Result<(), crate::Error> {
        Ok(ApiChainDao::set_chain_node(pool.as_ref(), chain_code, node_id).await?)
    }

    pub async fn detail_with_main_symbol(
        pool: &DbPool,
        main_symbol: &str,
    ) -> Result<Option<ApiChainEntity>, crate::Error> {
        Ok(ApiChainDao::detail_with_main_symbol(pool.as_ref(), main_symbol).await?)
    }
}
