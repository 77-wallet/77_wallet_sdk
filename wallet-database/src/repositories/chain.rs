use crate::{
    CoreDbPool,
    dao::chain::ChainDao,
    entities::chain::{ChainCreateVo, ChainEntity, ChainWithNode},
};

pub struct ChainRepo;

impl ChainRepo {
    pub async fn detail(
        pool: &CoreDbPool,
        chain_code: &str,
    ) -> Result<Option<ChainEntity>, crate::Error> {
        Ok(ChainDao::detail(pool.as_ref(), chain_code).await?)
    }

    pub async fn delete(
        pool: &CoreDbPool,
        chain_code: &str,
    ) -> Result<Option<ChainEntity>, crate::Error> {
        Ok(ChainDao::delete(pool.as_ref(), chain_code).await?)
    }

    pub async fn add(pool: &CoreDbPool, input: ChainCreateVo) -> Result<ChainEntity, crate::Error> {
        Ok(ChainDao::upsert(pool.as_ref(), input).await?)
    }

    pub async fn get_chain_list(pool: &CoreDbPool) -> Result<Vec<ChainEntity>, crate::Error> {
        Ok(ChainDao::list(pool.as_ref(), Some(1)).await?)
    }

    pub async fn get_chain_list_v2(pool: &CoreDbPool) -> Result<Vec<ChainEntity>, crate::Error> {
        Ok(ChainDao::list_v2(pool.as_ref(), Some(1)).await?)
    }

    pub async fn get_chain_node_list(
        pool: &CoreDbPool,
    ) -> Result<Vec<ChainWithNode>, crate::Error> {
        Ok(ChainDao::list_with_node_info(pool.as_ref()).await?)
    }

    pub async fn detail_with_node(
        pool: &CoreDbPool,
        chain_code: &str,
    ) -> Result<Option<ChainWithNode>, crate::Error> {
        Ok(ChainDao::chain_node_info(pool.as_ref(), chain_code).await?)
    }

    pub async fn detail_with_main_symbol(
        pool: &CoreDbPool,
        main_symbol: &str,
    ) -> Result<Option<ChainEntity>, crate::Error> {
        Ok(ChainDao::detail_with_main_symbol(pool.as_ref(), main_symbol).await?)
    }

    pub async fn toggle_chains_status(
        pool: &CoreDbPool,
        chain_codes: &[String],
    ) -> Result<Vec<ChainEntity>, crate::Error> {
        Ok(ChainDao::toggle_chains_status(pool.as_ref(), chain_codes).await?)
    }

    pub async fn upsert_multi_chain(
        pool: &CoreDbPool,
        input: Vec<ChainCreateVo>,
    ) -> Result<(), crate::Error> {
        ChainDao::upsert_multi_chain(pool.as_ref(), input).await
    }

    pub async fn user_select(
        pool: &CoreDbPool,
        chain_code: &str,
        node_id: &str,
    ) -> Result<(), crate::Error> {
        Ok(ChainDao::user_select(pool.as_ref(), chain_code, node_id).await?)
    }

    pub async fn set_chain_node_with_type(
        pool: &crate::CoreDbPool,
        chain_code: &str,
        node_id: &str,
        bind_type: crate::entities::api_chain::NodeBindType,
    ) -> Result<(), crate::Error> {
        Ok(ChainDao::set_chain_node_with_type(pool.as_ref(), chain_code, node_id, bind_type)
            .await?)
    }
}
