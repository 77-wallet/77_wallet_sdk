use crate::{
    CoreDbPool,
    entities::node::{NodeCreateVo, NodeEntity},
};

pub struct NodeRepo;

impl NodeRepo {
    pub async fn get_local_node_by_chain(
        pool: &CoreDbPool,
        chain_code: &str,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        Ok(NodeEntity::list(pool.as_ref(), &[chain_code.to_string()], Some(1), Some(1)).await?)
    }

    pub async fn list(
        pool: &CoreDbPool,
        is_local: Option<u8>,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        Ok(NodeEntity::list(pool.as_ref(), &[], is_local, None).await?)
    }

    pub async fn list_with_network(
        pool: &CoreDbPool,
        is_local: Option<u8>,
        network: Option<&str>,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        Ok(NodeEntity::list_with_network(pool.as_ref(), &[], is_local, None, network).await?)
    }

    pub async fn list_by_chain_with_network(
        pool: &CoreDbPool,
        chain_code: &str,
        network: Option<&str>,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        Ok(NodeEntity::list_with_network(
            pool.as_ref(),
            &[chain_code.to_string()],
            None,
            None,
            network,
        )
        .await?)
    }

    pub async fn upsert(pool: &CoreDbPool, req: NodeCreateVo) -> Result<NodeEntity, crate::Error> {
        Ok(NodeEntity::upsert(pool.as_ref(), req).await?)
    }

    pub async fn detail(
        pool: &CoreDbPool,
        node_id: &str,
    ) -> Result<Option<NodeEntity>, crate::Error> {
        let executor = pool.as_ref();
        Ok(NodeEntity::detail_by_node_id(executor, node_id).await?)
    }

    pub async fn disable_backend_not_in(
        pool: &CoreDbPool,
        chain_code: &str,
        backend_ids: &[String],
    ) -> Result<u64, crate::Error> {
        let executor = pool.as_ref();
        NodeEntity::disable_backend_not_in(executor, chain_code, backend_ids).await
    }

    pub async fn get_node_list_in_chain_codes(
        pool: &CoreDbPool,
        chain_codes: &[String],
        status: Option<u8>,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        Ok(NodeEntity::list(pool.as_ref(), chain_codes, None, status).await?)
    }
}
