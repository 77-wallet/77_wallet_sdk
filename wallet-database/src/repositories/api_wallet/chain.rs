use crate::{
    CoreDbPool,
    dao::api_chain::ApiChainDao,
    entities::api_chain::{ApiChainCreateVo, ApiChainEntity, ApiChainWithNode, NodeBindType},
};

pub struct ApiChainRepo;

impl ApiChainRepo {
    pub async fn get_chain_list(pool: &CoreDbPool) -> Result<Vec<ApiChainEntity>, crate::Error> {
        Ok(ApiChainDao::list(pool.as_ref(), Some(1)).await?)
    }

    pub async fn detail_with_node(
        pool: &CoreDbPool,
        chain_code: &str,
    ) -> Result<Option<ApiChainWithNode>, crate::Error> {
        Ok(ApiChainDao::chain_node_info(pool.as_ref(), chain_code).await?)
    }

    pub async fn detail(
        pool: &CoreDbPool,
        chain_code: &str,
    ) -> Result<Option<ApiChainEntity>, crate::Error> {
        Ok(ApiChainDao::detail(pool.as_ref(), chain_code).await?)
    }

    pub async fn add(pool: &CoreDbPool, input: ApiChainCreateVo) -> Result<(), crate::Error> {
        Ok(ApiChainDao::upsert(pool.as_ref(), input).await?)
    }

    pub async fn detail_with_main_symbol(
        pool: &CoreDbPool,
        main_symbol: &str,
    ) -> Result<Option<ApiChainEntity>, crate::Error> {
        Ok(ApiChainDao::detail_with_main_symbol(pool.as_ref(), main_symbol).await?)
    }

    pub async fn toggle_chains_status(
        pool: &CoreDbPool,
        chain_codes: &[String],
    ) -> Result<Vec<ApiChainEntity>, crate::Error> {
        Ok(ApiChainDao::toggle_chains_status(pool.as_ref(), chain_codes).await?)
    }

    pub async fn upsert_multi_chain(
        pool: &CoreDbPool,
        input: Vec<ApiChainCreateVo>,
    ) -> Result<(), crate::Error> {
        ApiChainDao::upsert_multi_chain(pool.as_ref(), input).await
    }

    // pub async fn set_chain_node_id_empty(
    //     pool: &CoreDbPool,
    //     node_id: &str,
    // ) -> Result<Vec<ApiChainEntity>, crate::Error> {
    //     ApiChainDao::set_chain_node_id_empty(pool.as_ref(), node_id).await
    // }

    pub async fn user_select(
        pool: &CoreDbPool,
        chain_code: &str,
        node_id: &str,
    ) -> Result<(), crate::Error> {
        Ok(ApiChainDao::user_select(pool.as_ref(), chain_code, node_id).await?)
    }

    pub async fn set_api_chain_node(
        pool: &CoreDbPool,
        chain_code: &str,
        node_id: &str,
    ) -> Result<Vec<ApiChainEntity>, crate::Error> {
        Ok(ApiChainDao::set_api_chain_node(pool.as_ref(), chain_code, node_id).await?)
    }

    pub async fn set_chain_node_with_type(
        pool: &CoreDbPool,
        chain_code: &str,
        node_id: &str,
        bind_type: NodeBindType,
    ) -> Result<(), crate::Error> {
        Ok(ApiChainDao::set_chain_node_with_type(pool.as_ref(), chain_code, node_id, bind_type)
            .await?)
    }

    pub async fn get_chain_node_list(
        pool: &CoreDbPool,
    ) -> Result<Vec<ApiChainWithNode>, crate::Error> {
        Ok(ApiChainDao::list_with_node_info(pool.as_ref()).await?)
    }

    pub async fn get_chain_list_all_status(
        pool: &CoreDbPool,
    ) -> Result<Vec<ApiChainEntity>, crate::Error> {
        Ok(ApiChainDao::list(pool.as_ref(), None).await?)
    }
}
