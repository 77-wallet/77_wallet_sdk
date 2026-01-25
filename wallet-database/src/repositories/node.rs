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
}
#[async_trait::async_trait]
pub trait NodeRepoTrait: super::TransactionTrait {
    async fn add(&mut self, input: NodeCreateVo) -> Result<NodeEntity, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, NodeEntity::upsert, input)
    }

    // async fn list(&mut self, is_local: Option<u8>) -> Result<Vec<NodeEntity>, crate::Error> {
    //     let executor = self.get_conn_or_tx()?;
    //     crate::execute_with_executor!(executor, NodeEntity::list, &[], is_local, None)
    // }

    async fn list_by_chain(
        &mut self,
        chain_code: &[String],
        is_local: Option<u8>,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, NodeEntity::list, chain_code, is_local, None)
    }

    async fn get_node_list_in_chain_codes(
        &mut self,
        chain_codes: &[String],
        status: Option<u8>,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, NodeEntity::list, chain_codes, None, status)
    }

    async fn delete(
        &mut self,
        // rpc_url: &str,
        // chain_code: &str,
        node_id: &str,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, NodeEntity::delete, node_id)
    }
}
